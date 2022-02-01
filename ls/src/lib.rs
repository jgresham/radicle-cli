use rad_common::{keys, profile, project};
use rad_terminal::components as term;
use rad_terminal::components::{Args, Error, Help};

pub const HELP: Help = Help {
    name: "ls",
    description: env!("CARGO_PKG_DESCRIPTION"),
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
USAGE
    rad ls [OPTIONS]

OPTIONS
    --help    Print help
"#,
};

pub struct Options {}

impl Args for Options {
    fn from_env() -> anyhow::Result<Self> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();

        if let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                _ => return Err(anyhow::anyhow!(arg.unexpected())),
            }
        }

        Ok(Options {})
    }
}

pub fn run(_options: Options) -> anyhow::Result<()> {
    let profile = profile::Profile::load()?;
    let sock = keys::ssh_auth_sock();
    let (_, storage) = keys::storage(&profile, sock)?;
    let projs = project::list(&storage)?;
    let mut table = term::Table::default();

    for (urn, meta, head) in projs {
        let head = head
            .map(|h| format!("{:.7}", h.to_string()))
            .unwrap_or_else(String::new);

        table.push([
            term::format::bold(meta.name),
            term::format::tertiary(urn),
            term::format::secondary(head),
            term::format::italic(meta.description),
        ]);
    }
    table.render();

    Ok(())
}