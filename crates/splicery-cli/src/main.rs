fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(splicery_cli::main(std::env::args_os()))
}
