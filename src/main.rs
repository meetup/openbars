extern crate clap;
#[macro_use]
extern crate error_chain;
extern crate handlebars;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate openapi;
extern crate walkdir;

use clap::{ArgMatches, App, Arg};
use handlebars::{Handlebars, Helper, RenderContext, RenderError};
use openapi::Spec;
use std::fs::{self, File, create_dir_all};
use std::path::{Path, MAIN_SEPARATOR};
use std::io::Read;
use walkdir::WalkDir;

pub mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Render(::handlebars::TemplateRenderError);
            Openapi(::openapi::errors::Error);
        }
    }
}

use errors::ResultExt;

fn apply_template<P, T>(template: P, target: T, spec: &Spec) -> errors::Result<()>
where
    P: AsRef<Path>,
    T: AsRef<Path>,
{
    // apply handlebars processing
    let apply = |path: &Path, hbs: &mut Handlebars| -> errors::Result<()> {

        let scratchpath = &format!("{}{}", &template.as_ref().to_str().unwrap(), MAIN_SEPARATOR)
                               [..];

        // path relatived based on scratch dir
        let localpath = path.to_str().unwrap().trim_left_matches(scratchpath);

        // eval path as template
        let evalpath = hbs.template_render(&localpath, &spec)
            .chain_err(|| format!("failed to render template {}", localpath))?;

        // rewritten path, based on target dir and eval path
        let targetpath = target.as_ref().join(evalpath);

        if path.is_dir() {
            fs::create_dir_all(targetpath)
                .chain_err(|| format!("failed to create directory {}", path.to_string_lossy()),)?
        } else {
            let mut file = File::open(path)?;
            let mut s = String::new();
            file.read_to_string(&mut s)?;
            let mut file = File::create(targetpath)?;
            hbs.template_renderw(&s, &spec, &mut file)?;
        }
        Ok(())
    };

    create_dir_all(target.as_ref())?;
    let mut hbs = bars();
    for entry in WalkDir::new(&template.as_ref())
            .into_iter()
            .skip(1)
            .filter_map(|e| e.ok()) {
        debug!("applying {:?}", entry.path().display());
        apply(entry.path(), &mut hbs)?
    }
    Ok(())
}

pub fn bars() -> Handlebars {
    let mut hbs = Handlebars::new();
    fn transform<F>(bars: &mut Handlebars, name: &str, f: F)
    where
        F: 'static + Fn(&str) -> String + Sync + Send,
    {
        bars.register_helper(
            name,
            Box::new(
                move |h: &Helper,
                      _: &Handlebars,
                      rc: &mut RenderContext|
                      -> ::std::result::Result<(), RenderError> {
                    let value = h.params().get(0).unwrap().value();
                    rc.writer.write(f(&format!("{}", value)).as_bytes())?;
                    Ok(())
                },
            ),
        );
    }

    transform(&mut hbs, "upper", str::to_uppercase);
    transform(&mut hbs, "lower", str::to_lowercase);

    hbs
}

fn run(args: ArgMatches) -> errors::Result<()> {
    let spec = openapi::from_path(
        args.value_of("spec")
            .expect("expected spec to be required"),
    )?;
    let template = args.value_of("template")
        .expect("expected template to be required");
    let target = args.value_of("target").unwrap_or(".");
    apply_template(template, target, &spec)?;
    Ok(())
}

fn main() {
    env_logger::init().unwrap();
    let args = App::new(env!("CARGO_PKG_NAME"))
       .version(env!("CARGO_PKG_VERSION"))
       .about("portable openapi handlebars templates")
       .arg(
           Arg::with_name("spec")
            .short("s")
            .long("spec")
            .value_name("spec")
            .takes_value(true)
            .required(true)
            .help(
                "path to open api specification"
            )
        )
        .arg(
          Arg::with_name("template")
            .short("t")
            .long("template")
            .value_name("template")
            .takes_value(true)
            .required(true)
            .help(
                "directory path containing handlebars template."
            )
        )
       .arg(
           Arg::with_name("target")
            .value_name("target")
            .help(
                "directory to write template output to. defaults to current working directory"
            )
        )
       .get_matches();

    if let Err(ref e) = run(args) {
        use std::io::Write;
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";
        writeln!(stderr, "error: {}", e).expect(errmsg);
        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).expect(errmsg);
        }
        // The backtrace is not always generated. Try to run this example
        // with `RUST_BACKTRACE=1`.
        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace).expect(errmsg);
        }

        ::std::process::exit(1);
    }
}
