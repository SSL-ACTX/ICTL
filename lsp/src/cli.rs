use std::ffi::OsString;

#[derive(Debug, PartialEq, Eq)]
pub enum AppMode {
    Serve,
    Help,
    Version,
}

pub fn parse_args<I>(args: I) -> AppMode
where
    I: IntoIterator<Item = OsString>,
{
    let mut iter = args.into_iter();
    // Skip program name
    let _ = iter.next();

    for arg in iter {
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => return AppMode::Help,
            "-V" | "--version" => return AppMode::Version,
            "--stdio" => return AppMode::Serve,
            _ => continue,
        }
    }

    AppMode::Serve
}

pub fn usage(program: &str) -> String {
    format!(
        "{program} - ICTL Language Server\n\nUSAGE:\n  {program} [OPTIONS]\n\nOPTIONS:\n  -h, --help       Print this help message and exit\n  -V, --version    Print version information and exit\n  --stdio          Run in stdio/LSP mode (default)\n",
        program = program
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn parse_args_defaults_to_serve() {
        let args = vec![OsString::from("ictl-lsp")];
        assert_eq!(parse_args(args), AppMode::Serve);
    }

    #[test]
    fn parse_args_help() {
        let args = vec![OsString::from("ictl-lsp"), OsString::from("--help")];
        assert_eq!(parse_args(args), AppMode::Help);

        let args = vec![OsString::from("ictl-lsp"), OsString::from("-h")];
        assert_eq!(parse_args(args), AppMode::Help);
    }

    #[test]
    fn parse_args_version() {
        let args = vec![OsString::from("ictl-lsp"), OsString::from("--version")];
        assert_eq!(parse_args(args), AppMode::Version);

        let args = vec![OsString::from("ictl-lsp"), OsString::from("-V")];
        assert_eq!(parse_args(args), AppMode::Version);
    }

    #[test]
    fn usage_contains_program_and_options() {
        let text = usage("ictl-lsp");
        assert!(text.contains("ictl-lsp - ICTL Language Server"));
        assert!(text.contains("--help"));
        assert!(text.contains("--version"));
        assert!(text.contains("--stdio"));
    }
}
