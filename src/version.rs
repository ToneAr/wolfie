pub(crate) const NAME: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " for ",
    env!("WOLFIE_SYSTEM_ID"),
    " (",
    env!("WOLFIE_BUILD_UID"),
    ")"
);
