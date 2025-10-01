use std::fmt::Display;

/// Util function for printing debug messages only when debug assertions are present,
/// messages will be printed to `stderr`
#[cfg(not(debug_assertions))]
pub fn dprint<S: Into<String> + Display>(_msg: S) {}

/// Util function for printing debug messages only when debug assertions are present,
/// messages will be printed to `stderr`
#[cfg(debug_assertions)]
pub fn dprint<S: Into<String> + Display>(msg: S) {
    eprintln!("[DEBUG] {}", msg);
}
