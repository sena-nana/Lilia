use lilia_xtask::{
    agent_debug, android, boundary, icons, installer_smoke, performance, pin, release, screenshot,
    Result, XtaskError,
};

fn main() {
    if let Err(error) = dispatch(std::env::args().skip(1).collect()) {
        lilia_xtask::print_error(&error);
        std::process::exit(if error.blocker { 2 } else { 1 });
    }
}

fn dispatch(arguments: Vec<String>) -> Result {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(usage());
    };
    let rest = &arguments[1..];
    match command {
        "verify" if rest.is_empty() => verify(),
        "boundary-check" if rest.is_empty() => boundary::check(),
        "pin-check" if rest.is_empty() => pin::check(),
        "agent-debug" if rest.is_empty() => agent_debug::run(),
        "screenshot" => screenshot::run(rest),
        "performance" if rest.is_empty() => performance::run(),
        "release" if rest.first().map(String::as_str) == Some("windows") => {
            release::windows(&rest[1..])
        }
        "installer-smoke" => installer_smoke::run(rest),
        "android" if rest.len() == 1 => android::run(&rest[0]),
        "icons" if rest.len() <= 1 => icons::run(rest.first().map(String::as_str)),
        _ => Err(usage()),
    }
}

fn verify() -> Result {
    boundary::check()?;
    pin::check()?;
    let root = lilia_xtask::repo_root()?;
    lilia_xtask::output(
        lilia_xtask::command("cargo").current_dir(&root).args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--no-deps",
        ]),
        "cargo metadata",
    )?;
    lilia_xtask::run(
        lilia_xtask::command("cargo")
            .current_dir(&root)
            .args(["test", "--locked", "--workspace"]),
        "workspace Rust tests",
    )?;
    lilia_xtask::run(
        lilia_xtask::command("cargo")
            .current_dir(root)
            .args(["check", "--locked", "--workspace"]),
        "workspace cargo check",
    )
}

fn usage() -> XtaskError {
    XtaskError::failure(
        "usage",
        "usage: cargo xtask <verify|boundary-check|pin-check|agent-debug|screenshot [--out <png>]|performance|release windows --tag <v...>|installer-smoke --tag <v...> [--path <installer>]|android doctor|android test|android build|android smoke|icons [source]>",
    )
}
