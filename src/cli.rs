//! Command-line arguments, parsed before the terminal is touched so
//! help, version and usage errors print to a normal shell.

use std::path::PathBuf;

use clap::Parser;

use crate::config;

#[derive(Debug, Parser)]
#[command(version, about)]
pub(crate) struct Cli {
    /// Config file to read and write.
    ///
    /// Defaults to $XDG_CONFIG_HOME/lazytimezone/config.toml, or
    /// ~/.config/lazytimezone/config.toml when that variable is unset.
    #[arg(short, long, value_name = "PATH")]
    config: Option<PathBuf>,
}

impl Cli {
    pub(crate) fn config_path(&self) -> Option<PathBuf> {
        self.config.clone().or_else(config::default_path)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
    use super::*;
    use crate::config;
    use clap::error::ErrorKind;
    use std::path::PathBuf;

    const PATH: &str = "/somewhere/lazytimezone.toml";

    #[test]
    fn a_config_flag_names_the_file_the_app_reads_and_writes() {
        let long = ["lazytimezone", "--config", PATH];
        let joined = ["lazytimezone", "--config=/somewhere/lazytimezone.toml"];
        let short = ["lazytimezone", "-c", PATH];

        for args in [long.as_slice(), joined.as_slice(), short.as_slice()] {
            let cli = Cli::try_parse_from(args).unwrap();
            assert_eq!(cli.config_path(), Some(PathBuf::from(PATH)));
        }
    }

    #[test]
    fn no_flag_falls_back_to_the_default_location() {
        let cli = Cli::try_parse_from(["lazytimezone"]).unwrap();

        assert_eq!(cli.config_path(), config::default_path());
    }

    #[test]
    fn a_config_flag_without_a_path_is_rejected() {
        let err = Cli::try_parse_from(["lazytimezone", "--config"]).unwrap_err();

        assert!(err.to_string().contains("--config"));
    }

    #[test]
    fn an_unknown_flag_is_rejected() {
        let err = Cli::try_parse_from(["lazytimezone", "--bogus"]).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    #[test]
    fn version_reports_the_crate_version() {
        let err = Cli::try_parse_from(["lazytimezone", "--version"]).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
        assert!(err.to_string().contains(env!("CARGO_PKG_VERSION")));
    }
}
