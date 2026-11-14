//! Last-resort crash guard. Code returns errors and never panics; this is the net for a panic
//! that escapes anyway: the hook logs it with its location. A panic inside a command is
//! contained by `Session::execute` and reported as an error, leaving the monitor state
//! unchanged. Needs `panic = "unwind"` (the default; never set "abort").

/// Log every panic, with its location, before the default hook prints it.
pub fn install_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("Pi Studio Dashboard internal error: {info}");
        default(info);
    }));
}
