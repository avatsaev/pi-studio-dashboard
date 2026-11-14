//! The desktop's implementations of the shell's service hooks. (The engine's hooks are built in
//! `owner.rs` from the client runtime and the I/O worker.)

use pistudiodashboard_shell::Services;

use crate::detect_local;

pub fn native() -> Services {
    Services { detect_local_daemon: Some(Box::new(detect_local::detect_from_environment)) }
}
