use std::process::ExitCode;

fn main() -> ExitCode {
    match kinobox_app::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("KinoBox не запустился: {err}");
            ExitCode::FAILURE
        }
    }
}
