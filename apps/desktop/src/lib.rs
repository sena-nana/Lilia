#![recursion_limit = "256"]

#[cfg(debug_assertions)]
mod agent_debug;
mod ask_user;
mod cli_import;
mod conversation_suggestions;
mod data_import;
#[cfg(debug_assertions)]
mod debug_fixture;
mod debug_timeline;
mod desktop;
mod document_editor;
mod host;
mod iab_panel;
mod iab_window;
mod markdown_images;
mod pending_import;
mod project_files_panel;
mod provider_ai_settings;
mod runtime_compat;
mod runtime_layout;
mod runtime_shell;
mod runtime_windows;
mod shell_integration;
mod single_instance;
mod startup_window;
mod storage;
pub mod target_ids;
mod task_session;
mod terminal_view;
mod text_editor_state;
mod updater;
mod windows_identity;

use desktop::{LiliaDesktopProgram, PRODUCT_NAME};
use nana_ui::{run_runtime, RuntimeWindowSettings};

#[no_mangle]
pub extern "system" fn liliacode_run(startup_window: isize) -> i32 {
    startup_window::register(startup_window);
    std::panic::catch_unwind(run_application).unwrap_or(101)
}

pub fn run() -> i32 {
    liliacode_run(0)
}

fn run_application() -> i32 {
    let startup_arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if pending_import::is_helper_request(&startup_arguments) {
        let home = match storage::lilia_home() {
            Ok(home) => home,
            Err(error) => {
                eprintln!("{error}");
                return 2;
            }
        };
        if let Err(error) = pending_import::run_helper(&home) {
            eprintln!("{error}");
            return 1;
        }
        return 0;
    }
    let launch_arguments = match cli_import::parse_startup(startup_arguments) {
        Ok(cli_import::StartupAction::Import(command)) => match cli_import::run(command) {
            Ok(result) => {
                if let Some(output) = result.stdout {
                    println!("{output}");
                }
                return result.exit_code;
            }
            Err(error) => {
                eprintln!("{error}");
                return 2;
            }
        },
        Ok(cli_import::StartupAction::Launch { arguments }) => arguments,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };

    let home = match storage::lilia_home() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    let request = lilia_desktop_application::DesktopCliRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        arguments: launch_arguments
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
        working_directory: std::env::current_dir().ok(),
    };
    let _instance = match single_instance::acquire(&home, storage::LILIA_INSTANCE_IDENTITY, request)
    {
        Ok(single_instance::InstanceDisposition::Primary(instance)) => instance,
        Ok(single_instance::InstanceDisposition::Forwarded(result)) => {
            if let Some(message) = result.message {
                if result.accepted {
                    println!("{message}");
                } else {
                    eprintln!("{message}");
                }
            }
            return result
                .exit_code
                .unwrap_or(if result.accepted { 0 } else { 2 });
        }
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    if let Err(error) = pending_import::activate_on_startup(&home) {
        eprintln!("pending LiliaCode data import was not activated: {error}");
        std::env::set_var("LILIA_IMPORT_ACTIVATION_FAILED", "1");
    }
    if let Err(error) = windows_identity::configure() {
        eprintln!("{error}");
        return 2;
    }

    let mut settings = RuntimeWindowSettings::new(PRODUCT_NAME)
        .initial_size(1180.0, 760.0)
        .minimum_size(780.0, 560.0);
    settings.transparent = true;
    if let Some(state) = storage::load_window_state(&home) {
        settings = state.apply_to_settings(settings);
    }

    match run_runtime::<LiliaDesktopProgram>(settings) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{PRODUCT_NAME} failed: {error}");
            1
        }
    }
}
