use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/tui-lookbook"));
    std::fs::create_dir_all(&out_dir)?;

    for story in jackin_tui::lookbook::stories() {
        let path = out_dir.join(jackin_tui::lookbook::story_svg_filename(story));
        std::fs::write(&path, jackin_tui::lookbook::render_story_to_svg(story))?;
        println!("{}", path.display());
    }

    Ok(())
}
