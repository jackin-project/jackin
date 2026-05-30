use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/tui-lookbook"));
    for path in jackin_tui::lookbook::write_story_svgs(&out_dir)? {
        println!("{}", path.display());
    }

    Ok(())
}
