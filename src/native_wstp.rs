use std::{
    io::{self, Write},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use wolfram_expr::{Expr, ExprKind, Symbol};
use wstp::{Link, Protocol, sys};

use crate::{
    highlighter::print_highlighted,
    kernel::{KernelExit, kernel_path},
    profiler::{profile_duration, profile_event},
    theme::ThemeHandle,
    wl::{WSTP_EVALUATE_USER_INPUT_WL, wolfram_string_literal},
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum KernelInputKind {
    Expression,
    String,
}

#[derive(Debug, Clone)]
pub(crate) struct KernelInputRequest {
    pub(crate) kind: KernelInputKind,
    pub(crate) prompt: String,
}

#[derive(Debug)]
enum KernelPacket {
    BeginDialog(i32),
    Call { function: i32, args: Expr },
    DisplayEnd,
    Display,
    EndDialog(i32),
    EnterExpression(Expr),
    EnterText(String),
    Evaluate(Expr),
    InputName(String),
    Input,
    InputString,
    Menu { id: i32, title: String },
    Message { symbol: String, tag: String },
    OutputName(String),
    Resume,
    Return(Expr),
    ReturnExpression(Expr),
    ReturnText(String),
    Suspend,
    Syntax(i32),
    Text(String),
    Unknown(i32),
}

type KernelInputHandler<'a> = dyn FnMut(&KernelInputRequest) -> Result<Option<String>> + 'a;

fn print_kernel_text(text: &str) -> Result<()> {
    print!("{text}");
    io::stdout().flush().context("failed to flush stdout")
}

pub(crate) struct WstpKernelClient {
    process: Child,
    link: Option<Link>,
    input_prompt: Option<String>,
}

impl WstpKernelClient {
    pub(crate) fn launch() -> Result<Self> {
        let start = Instant::now();
        let path = kernel_path()?;
        let mut link = Link::listen(Protocol::SharedMemory, "")
            .map_err(|err| anyhow!("failed to create WSTP listener: {err:?}"))?;
        let link_name = link.link_name();
        let spawn_start = Instant::now();
        let mut process = Command::new(path)
            .arg("-wstp")
            .arg("-linkprotocol")
            .arg("SharedMemory")
            .arg("-linkconnect")
            .arg("-linkname")
            .arg(&link_name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to launch WolframKernel in WSTP mode")?;
        profile_duration("wstp.launch.spawn", spawn_start.elapsed(), "");

        let activate_start = Instant::now();
        link.activate()
            .map_err(|err| anyhow!("failed to activate WSTP link: {err:?}"))?;
        profile_duration("wstp.launch.activate", activate_start.elapsed(), "");

        let prompt_start = Instant::now();
        let input_prompt = read_initial_input_name_packet(&mut link, &mut process)?;
        profile_duration(
            "wstp.launch.initial_prompt",
            prompt_start.elapsed(),
            input_prompt.as_str(),
        );
        profile_duration("wstp.launch.total", start.elapsed(), "");

        Ok(Self {
            process,
            link: Some(link),
            input_prompt: Some(input_prompt),
        })
    }

    pub(crate) fn evaluate_once(
        &mut self,
        input: &str,
        theme: Option<&ThemeHandle>,
        input_handler: Option<&mut KernelInputHandler<'_>>,
    ) -> Result<()> {
        let packets = self.evaluate_input_packets(input, input_handler)?;
        let input_prompt = last_input_name(&packets);
        render_packets(&packets, theme)?;
        if let Some(input_prompt) = input_prompt {
            self.input_prompt = Some(input_prompt);
        }
        Ok(())
    }

    pub(crate) fn input_prompt(&self) -> Option<&str> {
        self.input_prompt.as_deref()
    }

    pub(crate) fn evaluate_to_string(&mut self, input: &str) -> Result<String> {
        let expr = call(
            "System`ToString",
            vec![
                call("System`ToExpression", vec![Expr::string(input)]),
                symbol("System`InputForm"),
            ],
        );
        self.evaluate_packet_to_string(&expr)
    }

    fn evaluate_input_packets(
        &mut self,
        input: &str,
        input_handler: Option<&mut KernelInputHandler<'_>>,
    ) -> Result<Vec<KernelPacket>> {
        let start = Instant::now();
        let link = self.link.as_mut().context("WSTP link is closed")?;
        let input = wstp_user_input_text(input);
        put_enter_text_packet(link, &input)?;
        profile_duration("wstp.enter_text.sent", start.elapsed(), "");

        let packets = read_packets_until_return(
            link,
            &mut self.process,
            input_handler,
            true,
            "WSTP EnterTextPacket evaluation",
        )?;
        let output_bytes = packet_output_bytes(&packets);
        profile_duration(
            "wstp.enter_text.total",
            start.elapsed(),
            format!("bytes={output_bytes}"),
        );
        Ok(packets)
    }

    fn evaluate_packet_to_string(&mut self, expr: &Expr) -> Result<String> {
        let start = Instant::now();
        let link = self.link.as_mut().context("WSTP link is closed")?;
        link.put_eval_packet(expr)
            .map_err(|err| anyhow!("failed to send WSTP EvaluatePacket: {err:?}"))?;
        link.flush()
            .map_err(|err| anyhow!("failed to flush WSTP link: {err:?}"))?;
        profile_duration("wstp.eval.sent", start.elapsed(), "");

        let packets = read_packets_until_return(
            link,
            &mut self.process,
            None,
            false,
            "WSTP EvaluatePacket query",
        )?;
        let text = packets
            .iter()
            .rev()
            .find_map(packet_text_result)
            .unwrap_or_default();
        profile_duration(
            "wstp.eval.total",
            start.elapsed(),
            format!("bytes={}", text.len()),
        );
        Ok(text)
    }

    fn child_exit_code_after_link_error(process: &mut Child) -> Option<i32> {
        for _ in 0..20 {
            match process.try_wait() {
                Ok(Some(status)) => return status.code(),
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(_) => return None,
            }
        }
        None
    }

    fn stop_child(&mut self) {
        if let Some(link) = self.link.take() {
            std::mem::forget(link);
        }

        for _ in 0..20 {
            if self.process.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl Drop for WstpKernelClient {
    fn drop(&mut self) {
        self.stop_child();
    }
}

fn wstp_user_input_text(input: &str) -> String {
    let input = if input.contains("Input[") || input.contains("InputString[") {
        WSTP_EVALUATE_USER_INPUT_WL.replace(
            "WOLFRAMCLIINPUTPLACEHOLDER",
            &wolfram_string_literal(input),
        )
    } else {
        input.to_owned()
    };
    input_form_string_evaluation(&input)
}

fn input_form_string_evaluation(input: &str) -> String {
    format!("ToString[{input}, InputForm]")
}

fn put_enter_text_packet(link: &mut Link, input: &str) -> Result<()> {
    link.put_function("System`EnterTextPacket", 1)
        .map_err(|err| anyhow!("failed to begin WSTP EnterTextPacket: {err:?}"))?;
    link.put_str(input)
        .map_err(|err| anyhow!("failed to write WSTP EnterTextPacket text: {err:?}"))?;
    link.end_packet()
        .map_err(|err| anyhow!("failed to finish WSTP EnterTextPacket: {err:?}"))?;
    link.flush()
        .map_err(|err| anyhow!("failed to flush WSTP EnterTextPacket: {err:?}"))
}

fn read_initial_input_name_packet(link: &mut Link, process: &mut Child) -> Result<String> {
    loop {
        let packet_id = next_packet_id(link, process, "initial prompt")?;
        let packet = read_packet_payload(link, packet_id)?;
        if let KernelPacket::InputName(prompt) = packet {
            finish_packet(link, "initial InputNamePacket")?;
            return Ok(prompt);
        }
        if matches!(packet, KernelPacket::Input | KernelPacket::InputString) {
            bail!(
                "kernel sent {} before the initial InputNamePacket",
                packet_name(packet_id)
            );
        }
        finish_packet(link, "initial packet")?;
    }
}

fn read_packets_until_return(
    link: &mut Link,
    process: &mut Child,
    mut input_handler: Option<&mut KernelInputHandler<'_>>,
    read_next_input_name: bool,
    operation: &str,
) -> Result<Vec<KernelPacket>> {
    let mut packets = Vec::new();

    loop {
        let packet_id = next_packet_id(link, process, operation)?;
        let packet = read_packet_payload(link, packet_id)?;
        trace_packet(operation, &packet);
        let terminal = packet_is_terminal(&packet);
        let next_prompt_after_result = read_next_input_name && matches!(packet, KernelPacket::InputName(_));
        let input_request = match packet {
            KernelPacket::Input => Some(KernelInputRequest {
                kind: KernelInputKind::Expression,
                prompt: input_request_prompt(&packets),
            }),
            KernelPacket::InputString => Some(KernelInputRequest {
                kind: KernelInputKind::String,
                prompt: input_request_prompt(&packets),
            }),
            _ => None,
        };

        packets.push(packet);

        if let Some(request) = input_request {
            let response = match input_handler.as_deref_mut() {
                Some(handler) => handler(&request)?,
                None => bail!("kernel requested input during {operation}, but no input handler is available"),
            };
            let response = response.context("kernel input was cancelled")?;
            send_input_response(link, &request, &response)?;
            continue;
        }

        if next_prompt_after_result {
            finish_packet(link, "WSTP InputNamePacket")?;
            return Ok(packets);
        }

        if terminal {
            if read_next_input_name {
                finish_packet(link, "WSTP terminal packet")?;
                continue;
            }
            return Ok(packets);
        }

        finish_packet(link, "WSTP packet")?;
    }
}

fn next_packet_id(link: &mut Link, process: &mut Child, operation: &str) -> Result<i32> {
    match link.raw_next_packet() {
        Ok(packet_id) => Ok(packet_id),
        Err(err) => {
            if let Some(code) = WstpKernelClient::child_exit_code_after_link_error(process) {
                return Err(KernelExit::new(code).into());
            }
            Err(anyhow!("failed to read packet during {operation}: {err:?}"))
        }
    }
}

fn send_input_response(
    link: &mut Link,
    request: &KernelInputRequest,
    response: &str,
) -> Result<()> {
    let response = response.trim_end_matches(['\r', '\n']);
    finish_packet(link, "WSTP input request packet")?;
    match request.kind {
        KernelInputKind::Expression => {
            put_enter_text_packet(link, response)
                .map_err(|err| anyhow!("failed to send WSTP InputPacket response: {err:?}"))?;
        }
        KernelInputKind::String => {
            link.put_str(response)
                .map_err(|err| anyhow!("failed to send WSTP InputStringPacket response: {err:?}"))?;
            link.end_packet().map_err(|err| {
                anyhow!("failed to finish WSTP InputStringPacket response packet: {err:?}")
            })?;
            link.flush().map_err(|err| {
                anyhow!("failed to flush WSTP InputStringPacket response: {err:?}")
            })?;
        }
    }
    Ok(())
}

fn finish_packet(link: &mut Link, context: &str) -> Result<()> {
    link.new_packet()
        .map_err(|err| anyhow!("failed to finish {context}: {err:?}"))
}

fn read_packet_payload(link: &mut Link, packet_id: i32) -> Result<KernelPacket> {
    let packet = match packet_id {
        sys::BEGINDLGPKT => KernelPacket::BeginDialog(read_i32(link, "BeginDialogPacket")?),
        sys::CALLPKT => KernelPacket::Call {
            function: read_i32(link, "CallPacket function")?,
            args: read_expr(link, "CallPacket arguments")?,
        },
        sys::DISPLAYENDPKT => KernelPacket::DisplayEnd,
        sys::DISPLAYPKT => KernelPacket::Display,
        sys::ENDDLGPKT => KernelPacket::EndDialog(read_i32(link, "EndDialogPacket")?),
        sys::ENTEREXPRPKT => KernelPacket::EnterExpression(read_expr(link, "EnterExpressionPacket")?),
        sys::ENTERTEXTPKT => KernelPacket::EnterText(read_string(link, "EnterTextPacket")?),
        sys::EVALUATEPKT => KernelPacket::Evaluate(read_expr(link, "EvaluatePacket")?),
        sys::INPUTNAMEPKT => KernelPacket::InputName(read_string(link, "InputNamePacket")?),
        sys::INPUTPKT => KernelPacket::Input,
        sys::INPUTSTRPKT => KernelPacket::InputString,
        sys::MENUPKT => KernelPacket::Menu {
            id: read_i32(link, "MenuPacket id")?,
            title: read_string(link, "MenuPacket title")?,
        },
        sys::MESSAGEPKT => KernelPacket::Message {
            symbol: read_symbol(link, "MessagePacket symbol")?,
            tag: read_string(link, "MessagePacket tag")?,
        },
        sys::OUTPUTNAMEPKT => KernelPacket::OutputName(read_string(link, "OutputNamePacket")?),
        sys::RESUMEPKT => KernelPacket::Resume,
        sys::RETURNEXPRPKT => KernelPacket::ReturnExpression(read_expr(link, "ReturnExpressionPacket")?),
        sys::RETURNPKT => KernelPacket::Return(read_expr(link, "ReturnPacket")?),
        sys::RETURNTEXTPKT => KernelPacket::ReturnText(read_string(link, "ReturnTextPacket")?),
        sys::SUSPENDPKT => KernelPacket::Suspend,
        sys::SYNTAXPKT => KernelPacket::Syntax(read_i32(link, "SyntaxPacket")?),
        sys::TEXTPKT => KernelPacket::Text(read_string(link, "TextPacket")?),
        unknown => KernelPacket::Unknown(unknown),
    };
    Ok(packet)
}

fn read_string(link: &mut Link, context: &str) -> Result<String> {
    link.get_string()
        .map_err(|err| anyhow!("failed to read {context} string: {err:?}"))
}

fn read_symbol(link: &mut Link, context: &str) -> Result<String> {
    link.get_symbol_ref()
        .map(|symbol| symbol.as_str().to_owned())
        .map_err(|err| anyhow!("failed to read {context}: {err:?}"))
}

fn read_i32(link: &mut Link, context: &str) -> Result<i32> {
    link.get_i32()
        .map_err(|err| anyhow!("failed to read {context} integer: {err:?}"))
}

fn read_expr(link: &mut Link, context: &str) -> Result<Expr> {
    link.get_expr()
        .map_err(|err| anyhow!("failed to read {context} expression: {err:?}"))
}

fn packet_is_terminal(packet: &KernelPacket) -> bool {
    matches!(
        packet,
        KernelPacket::Return(_)
            | KernelPacket::ReturnExpression(_)
            | KernelPacket::ReturnText(_)
            | KernelPacket::Syntax(_)
    )
}

fn input_request_prompt(packets: &[KernelPacket]) -> String {
    packets
        .iter()
        .rev()
        .find_map(|packet| match packet {
            KernelPacket::Text(text) if !text.ends_with('\n') => Some(text.clone()),
            KernelPacket::InputName(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn last_input_name(packets: &[KernelPacket]) -> Option<String> {
    packets.iter().rev().find_map(|packet| match packet {
        KernelPacket::InputName(text) => Some(text.clone()),
        _ => None,
    })
}

fn packet_name(packet_id: i32) -> &'static str {
    match packet_id {
        sys::BEGINDLGPKT => "BeginDialogPacket",
        sys::CALLPKT => "CallPacket",
        sys::DISPLAYENDPKT => "DisplayEndPacket",
        sys::DISPLAYPKT => "DisplayPacket",
        sys::ENDDLGPKT => "EndDialogPacket",
        sys::ENTEREXPRPKT => "EnterExpressionPacket",
        sys::ENTERTEXTPKT => "EnterTextPacket",
        sys::EVALUATEPKT => "EvaluatePacket",
        sys::INPUTNAMEPKT => "InputNamePacket",
        sys::INPUTPKT => "InputPacket",
        sys::INPUTSTRPKT => "InputStringPacket",
        sys::MENUPKT => "MenuPacket",
        sys::MESSAGEPKT => "MessagePacket",
        sys::OUTPUTNAMEPKT => "OutputNamePacket",
        sys::RESUMEPKT => "ResumePacket",
        sys::RETURNEXPRPKT => "ReturnExpressionPacket",
        sys::RETURNPKT => "ReturnPacket",
        sys::RETURNTEXTPKT => "ReturnTextPacket",
        sys::SUSPENDPKT => "SuspendPacket",
        sys::SYNTAXPKT => "SyntaxPacket",
        sys::TEXTPKT => "TextPacket",
        _ => "unknown packet",
    }
}

fn packet_text_result(packet: &KernelPacket) -> Option<String> {
    match packet {
        KernelPacket::Return(expr) | KernelPacket::ReturnExpression(expr) => {
            Some(expr_string_value(expr).unwrap_or_else(|| expr.to_string()))
        }
        KernelPacket::ReturnText(text) => Some(text.clone()),
        _ => None,
    }
}

fn expr_string_value(expr: &Expr) -> Option<String> {
    match expr.kind() {
        ExprKind::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn packet_output_bytes(packets: &[KernelPacket]) -> usize {
    packets
        .iter()
        .map(|packet| match packet {
            KernelPacket::Text(text)
            | KernelPacket::ReturnText(text)
            | KernelPacket::InputName(text)
            | KernelPacket::OutputName(text) => text.len(),
            KernelPacket::Return(expr) | KernelPacket::ReturnExpression(expr) => expr.to_string().len(),
            _ => 0,
        })
        .sum()
}

fn trace_packet(operation: &str, packet: &KernelPacket) {
    profile_event(format!("wstp.packet\t{operation}\t{}", packet_summary(packet)));
}

fn packet_summary(packet: &KernelPacket) -> String {
    match packet {
        KernelPacket::BeginDialog(id) => format!("BeginDialogPacket[{id}]"),
        KernelPacket::Call { function, args } => format!("CallPacket[{function}, {args}]"),
        KernelPacket::DisplayEnd => "DisplayEndPacket[]".to_owned(),
        KernelPacket::Display => "DisplayPacket[]".to_owned(),
        KernelPacket::EndDialog(id) => format!("EndDialogPacket[{id}]"),
        KernelPacket::EnterExpression(expr) => format!("EnterExpressionPacket[{expr}]"),
        KernelPacket::EnterText(text) => format!("EnterTextPacket[{}]", debug_text(text)),
        KernelPacket::Evaluate(expr) => format!("EvaluatePacket[{expr}]"),
        KernelPacket::InputName(text) => format!("InputNamePacket[{}]", debug_text(text)),
        KernelPacket::Input => "InputPacket[]".to_owned(),
        KernelPacket::InputString => "InputStringPacket[]".to_owned(),
        KernelPacket::Menu { id, title } => format!("MenuPacket[{id}, {}]", debug_text(title)),
        KernelPacket::Message { symbol, tag } => {
            format!("MessagePacket[{symbol}, {}]", debug_text(tag))
        }
        KernelPacket::OutputName(text) => format!("OutputNamePacket[{}]", debug_text(text)),
        KernelPacket::Resume => "ResumePacket[]".to_owned(),
        KernelPacket::Return(expr) => format!("ReturnPacket[{expr}]"),
        KernelPacket::ReturnExpression(expr) => format!("ReturnExpressionPacket[{expr}]"),
        KernelPacket::ReturnText(text) => format!("ReturnTextPacket[{}]", debug_text(text)),
        KernelPacket::Suspend => "SuspendPacket[]".to_owned(),
        KernelPacket::Syntax(position) => format!("SyntaxPacket[{position}]"),
        KernelPacket::Text(text) => format!("TextPacket[{}]", debug_text(text)),
        KernelPacket::Unknown(id) => format!("UnknownPacket[{id}]"),
    }
}

fn debug_text(text: &str) -> String {
    format!("{text:?}")
}

fn render_packets(packets: &[KernelPacket], theme: Option<&ThemeHandle>) -> Result<()> {
    let mut output_name: Option<&str> = None;
    let mut text_without_trailing_newline = false;

    for (index, packet) in packets.iter().enumerate() {
        match packet {
            KernelPacket::Text(text) => {
                if text_is_input_prompt(packets, index) {
                    text_without_trailing_newline = false;
                    continue;
                }
                print_kernel_text(text)?;
                text_without_trailing_newline = !text.ends_with('\n');
            }
            KernelPacket::Message { symbol, tag } => {
                let _ = (symbol, tag);
            }
            KernelPacket::OutputName(name) => output_name = Some(name),
            KernelPacket::Return(expr) | KernelPacket::ReturnExpression(expr) => {
                if text_without_trailing_newline {
                    print_kernel_text("\n")?;
                    text_without_trailing_newline = false;
                }
                let text = return_expression_text(expr);
                render_return_text(&text, output_name.take(), theme)?;
            }
            KernelPacket::ReturnText(text) => {
                if text_without_trailing_newline {
                    print_kernel_text("\n")?;
                    text_without_trailing_newline = false;
                }
                render_return_text(text, output_name.take(), theme)?;
            }
            KernelPacket::Syntax(position) => {
                print_kernel_text(&format!("Syntax error at position {position}\n"))?;
            }
            KernelPacket::BeginDialog(id) => {
                print_kernel_text(&format!("BeginDialogPacket[{id}]\n"))?;
            }
            KernelPacket::EndDialog(id) => {
                print_kernel_text(&format!("EndDialogPacket[{id}]\n"))?;
            }
            KernelPacket::Menu { id, title } => {
                print_kernel_text(&format!("MenuPacket[{id}, {title}]\n"))?;
            }
            KernelPacket::Call { function, args } => {
                print_kernel_text(&format!("CallPacket[{function}, {args}]\n"))?;
            }
            KernelPacket::Unknown(id) => {
                print_kernel_text(&format!("Unknown WSTP packet {id}\n"))?;
            }
            KernelPacket::EnterExpression(expr) => {
                print_kernel_text(&format!("EnterExpressionPacket[{expr}]\n"))?;
            }
            KernelPacket::EnterText(text) => {
                print_kernel_text(&format!("EnterTextPacket[{text}]\n"))?;
            }
            KernelPacket::Evaluate(expr) => {
                print_kernel_text(&format!("EvaluatePacket[{expr}]\n"))?;
            }
            KernelPacket::Display
            | KernelPacket::DisplayEnd
            | KernelPacket::Input
            | KernelPacket::InputName(_)
            | KernelPacket::InputString
            | KernelPacket::Resume
            | KernelPacket::Suspend => {}
        }
    }

    Ok(())
}

fn return_expression_text(expr: &Expr) -> String {
    expr.to_string()
}

fn text_is_input_prompt(packets: &[KernelPacket], index: usize) -> bool {
    matches!(
        packets.get(index + 1),
        Some(KernelPacket::Input | KernelPacket::InputString)
    ) && matches!(packets.get(index), Some(KernelPacket::Text(text)) if !text.ends_with('\n'))
}

fn render_return_text(
    text: &str,
    output_name: Option<&str>,
    theme: Option<&ThemeHandle>,
) -> Result<()> {
    if return_text_is_suppressed(text) {
        return Ok(());
    }

    if let Some(output_name) = output_name {
        print_kernel_text(output_name)?;
    }

    if let Some(theme) = theme {
        print_highlighted(text, theme.current());
    } else {
        println!("{text}");
    }
    Ok(())
}

fn return_text_is_suppressed(text: &str) -> bool {
    text.is_empty() || text == "Null"
}

fn symbol(name: &str) -> Expr {
    Expr::symbol(Symbol::try_new(name).expect("internal symbol names are qualified"))
}

fn call(head: &str, args: Vec<Expr>) -> Expr {
    Expr::normal(symbol(head), args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_expression_text_preserves_string_quotes() {
        assert_eq!(return_expression_text(&Expr::string("abc")), "\"abc\"");
    }

    #[test]
    fn user_input_is_evaluated_as_input_form_string() {
        assert_eq!(
            wstp_user_input_text("\"abc\""),
            "ToString[\"abc\", InputForm]"
        );
    }

    #[test]
    fn null_return_text_is_suppressed() {
        assert!(return_text_is_suppressed(""));
        assert!(return_text_is_suppressed("Null"));
        assert!(!return_text_is_suppressed("\"Null\""));
    }
}
