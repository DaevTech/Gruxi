use gruxi::{core::command_line_args::{check_for_command_line_actions, get_command_line_args}};
use qsu::rt::RunCtx;


fn main() {
    let logo = r#"
  ________                   .__
 /  _____/______ __ _____  __|__|
/   \  __\_  __ \  |  \  \/  /  |
\    \_\  \  | \/  |  />    <|  |
 \______  /__|  |____//__/\_ \__|
        \/     WEBSERVER    \/
"#;
    println!("{}", logo);

    // Check the prerequisites for running Gruxi, such as checking if the required files and directories exist and are accessible
    if let Err(e) = gruxi::file::file_requirements::check_file_requirements() {
        eprintln!("File requirements check failed: {}", e);
        std::process::exit(1);
    }

    // Process command line arguments that should be executed before starting the service
    check_for_command_line_actions();
    let cli = get_command_line_args();

    // If we are running as a service
    let run_as_service = cli.get_flag("service");
    let mut ctx = RunCtx::new("gruxi").log_init(false);
    if run_as_service {
        ctx = ctx.service();
    }

    if let Err(e) = ctx.run_tokio(None, Box::new(gruxi::core::service::svcevt_handler), Box::new(gruxi::core::service::GruxiService)) {
        eprintln!("Gruxi service error: {:?}", e);
        std::process::exit(1);
    }
}
