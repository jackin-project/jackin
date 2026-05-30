use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

const USAGE: &str = "usage: tui-lookbook [out-dir] | tui-lookbook --check <dir>";
const CHECK_USAGE: &str = "usage: tui-lookbook --check <docs/public/tui-lookbook>";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let Some(first) = args.next() else {
        return write_svgs(PathBuf::from("target/tui-lookbook"));
    };

    if first == OsStr::new("--check") {
        let Some(dir) = args.next() else {
            return Err(CHECK_USAGE.into());
        };
        if args.next().is_some() {
            return Err(CHECK_USAGE.into());
        }
        return check_svgs(PathBuf::from(dir));
    }

    if args.next().is_some() {
        return Err(USAGE.into());
    }
    write_svgs(PathBuf::from(first))
}

fn write_svgs(out_dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    for path in jackin_tui::lookbook::write_story_svgs(&out_dir)? {
        println!("{}", path.display());
    }

    Ok(())
}

fn check_svgs(dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let expected = expected_svg_names();
    let actual = actual_svg_names(&dir)?;
    let mut failures = Vec::new();

    for missing in expected.difference(&actual) {
        failures.push(format!("missing generated preview: {missing}"));
    }
    for stale in actual.difference(&expected) {
        failures.push(format!("stale generated preview: {stale}"));
    }

    for story in jackin_tui::lookbook::stories() {
        let filename = jackin_tui::lookbook::story_svg_filename(story);
        let path = dir.join(&filename);
        if !path.exists() {
            continue;
        }
        let committed = fs::read_to_string(&path)?;
        let rendered = jackin_tui::lookbook::render_story_to_svg(story);
        if committed != rendered {
            failures.push(format!(
                "generated preview is stale: {}",
                path.display()
            ));
        }
    }

    if failures.is_empty() {
        println!("tui lookbook previews are current");
        Ok(())
    } else {
        for failure in &failures {
            eprintln!("{failure}");
        }
        Err(concat!(
            "tui lookbook previews are out of date; regenerate with ",
            "`cargo run -p jackin-tui --bin tui-lookbook -- docs/public/tui-lookbook`",
        )
        .into())
    }
}

fn expected_svg_names() -> BTreeSet<String> {
    jackin_tui::lookbook::stories()
        .into_iter()
        .map(jackin_tui::lookbook::story_svg_filename)
        .collect()
}

fn actual_svg_names(dir: &Path) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("svg")) {
            continue;
        }
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            return Err(format!("non-UTF-8 lookbook preview path: {}", path.display()).into());
        };
        names.insert(name.to_owned());
    }
    Ok(names)
}
