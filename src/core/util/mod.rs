//! Small helpers: paths, logging, wide strings, single-instance guard, Explorer.

mod dialogs;
mod instance;
mod logging;
mod paths;
mod shell;
mod wide;

pub use dialogs::show_message;
pub use instance::acquire_single_instance;
pub use logging::{init_logging, log};
pub use paths::{ensure_data_dir, log_path};
pub use shell::open_path;
pub use wide::{set_wide, to_wide, truncate};
