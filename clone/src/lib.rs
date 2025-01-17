use std::convert::TryFrom;
use std::ffi::OsString;
use std::str::FromStr;

use anyhow::anyhow;
use librad::git::tracking;
use librad::git::Urn;
use url::Url;

use rad_common::seed::{self, SeedOptions};
use rad_common::{git, keys, profile, project};
use rad_terminal::args::{Args, Error, Help};
use rad_terminal::components as term;

pub const HELP: Help = Help {
    name: "clone",
    description: env!("CARGO_PKG_DESCRIPTION"),
    version: env!("CARGO_PKG_VERSION"),
    usage: r#"
Usage

    rad clone <urn | url> [--seed <host>] [<option>...]

Options

    --seed <host>   Seed to clone from
    --help          Print help

"#,
};

#[derive(Debug)]
enum Origin {
    Radicle(project::Origin),
    Git(Url),
}

#[derive(Debug)]
pub struct Options {
    origin: Origin,
}

impl Args for Options {
    fn from_args(args: Vec<OsString>) -> anyhow::Result<(Self, Vec<OsString>)> {
        use lexopt::prelude::*;

        let (SeedOptions(seed), unparsed) = SeedOptions::from_args(args)?;
        let mut parser = lexopt::Parser::from_args(unparsed);
        let mut origin: Option<Origin> = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("help") => {
                    return Err(Error::Help.into());
                }
                Value(val) if origin.is_none() => {
                    let val = val.to_string_lossy();
                    match Url::parse(&val) {
                        Ok(url) if url.scheme() == project::URL_SCHEME => {
                            let o = project::Origin::try_from(url)?;
                            origin = Some(Origin::Radicle(o));
                        }
                        Ok(url) => {
                            origin = Some(Origin::Git(url));
                        }
                        Err(_) => {
                            let urn = Urn::from_str(&val)?;
                            origin = Some(Origin::Radicle(project::Origin::from_urn(urn)));
                        }
                    }
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }
        let origin = origin.ok_or_else(|| {
            anyhow!("to clone, a URN or URL must be provided; see `rad clone --help`")
        })?;

        match (&origin, seed) {
            (Origin::Radicle(o), Some(_)) if o.seed.is_some() => {
                anyhow::bail!("`--seed` cannot be specified when a URL is given as origin");
            }
            (Origin::Git(_), Some(_)) => {
                anyhow::bail!("`--seed` cannot be specified when a Git URL is given");
            }
            _ => {}
        }

        Ok((Options { origin }, vec![]))
    }
}

pub fn run(options: Options) -> anyhow::Result<()> {
    match options.origin {
        Origin::Radicle(origin) => {
            clone_project(origin.urn, origin.seed)?;
        }
        Origin::Git(url) => {
            clone_repository(url)?;
        }
    }
    Ok(())
}

pub fn clone_project(urn: Urn, seed: Option<seed::Address>) -> anyhow::Result<()> {
    rad_sync::run(rad_sync::Options {
        fetch: true,
        refs: rad_sync::Refs::All,
        origin: Some(project::Origin {
            urn: urn.clone(),
            seed: seed.clone(),
        }),
        seed: None,
        identity: true,
        push_self: false,
        verbose: false,
    })?;
    let path = rad_checkout::execute(rad_checkout::Options { urn: urn.clone() })?;

    if let Some(seed_url) = seed.map(|s| s.url()) {
        seed::set_seed(&seed_url, seed::Scope::Local(&path))?;
        term::success!(
            "Local repository seed for {} set to {}",
            term::format::highlight(path.display()),
            term::format::highlight(seed_url)
        );
    }

    let profile = profile::default()?;
    let sock = keys::ssh_auth_sock();
    let (_, storage) = keys::storage(&profile, sock)?;
    let cfg = tracking::config::Config::default();
    let project = project::get(&storage, &urn)?
        .ok_or_else(|| anyhow!("couldn't load project {} from local state", urn))?;

    // Track all project delegates.
    for peer in project.remotes {
        tracking::track(
            &storage,
            &urn,
            Some(peer),
            cfg.clone(),
            tracking::policy::Track::Any,
        )??;
    }
    term::success!("Tracking for project delegates configured");

    term::headline(&format!(
        "🌱 Project clone successful under ./{}",
        term::format::highlight(path.file_name().unwrap_or_default().to_string_lossy())
    ));

    Ok(())
}

pub fn clone_repository(url: Url) -> anyhow::Result<()> {
    let proj = url
        .path_segments()
        .ok_or(anyhow!("couldn't get segments of URL"))?
        .last()
        .ok_or(anyhow!("couldn't get last segment of URL"))?;
    let proj = proj.strip_suffix(".git").unwrap_or(proj);
    let destination = std::env::current_dir()?.join(proj);

    let spinner = term::spinner(&format!(
        "Cloning git repository {}...",
        term::format::highlight(&url)
    ));
    git::clone(url.as_str(), &destination)?;
    spinner.finish();

    if term::confirm(format!(
        "Initialize new 🌱 project in {}?",
        term::format::highlight(destination.display())
    )) {
        term::blank();
        rad_init::execute(destination.as_path())?;
    }
    Ok(())
}
