//! Copies Phosphor icons into an icon folder as white front layers plus duo back layers.
//!
//! ```text
//! cargo run -p egui-icon-pack-build --example import_phosphor -- \
//!     <phosphor-core>/assets assets/icons cpu hard-drive gear
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use egui_icon_pack_build::phosphor::import;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(core_assets), Some(out_dir)) = (
        args.next().map(PathBuf::from),
        args.next().map(PathBuf::from),
    ) else {
        eprintln!("usage: import_phosphor <phosphor-core>/assets <out-dir> <icon-name>...");
        return ExitCode::FAILURE;
    };
    let mut failed = false;
    for name in args {
        match import(&core_assets, &out_dir, &name) {
            Ok(done) => println!("{name}: regular{}", if done.duo { " + duo" } else { "" }),
            Err(err) => {
                eprintln!("{name}: {err}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
