//! The `vcxtoolkit` command line.
//!
//! `doctor` (alias `info`) opens the VCX over USB and reads `GetInfo`: a safe
//! pre-flight that opens no vehicle channel. `collect` builds the module-read
//! collection plan but holds the vehicle send. The logic lives in
//! [`vcxtoolkit::cli`]; this binary only forwards the process arguments.

use std::io::Write as _;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    let code = vcxtoolkit::cli::run(&args, &mut handle);
    let _ = handle.flush();
    std::process::exit(code);
}
