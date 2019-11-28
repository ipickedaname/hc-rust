pub mod constants;
pub mod get_omer;
pub mod print;
use crate::args::types::{AppError, MainArgs};
pub use get_omer::get_omer;
pub use print::*;

pub trait Runnable<T: Printable> {
    fn run(&self, args: &MainArgs) -> Result<T, AppError>;
}

pub trait Printable {
    fn print(&self, args: MainArgs) -> Result<(), AppError>;
}
