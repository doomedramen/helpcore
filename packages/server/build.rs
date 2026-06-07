use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-env-changed=HELPCORE_EMBED_WEB_DIR");

    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set")).join("embedded_web.rs");
    let Some(web_dir) = env::var_os("HELPCORE_EMBED_WEB_DIR").map(PathBuf::from) else {
        fs::write(output, "pub static FILES: &[(&str, &[u8])] = &[];\n")
            .expect("write empty embedded web manifest");
        return;
    };

    let web_dir = web_dir
        .canonicalize()
        .unwrap_or_else(|error| panic!("cannot read web export {}: {error}", web_dir.display()));
    if !web_dir.join("index.html").is_file() {
        panic!(
            "web export {} does not contain index.html",
            web_dir.display()
        );
    }

    println!("cargo:rerun-if-changed={}", web_dir.display());
    let mut files = Vec::new();
    collect_files(&web_dir, &web_dir, &mut files);
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut generated = String::from("pub static FILES: &[(&str, &[u8])] = &[\n");
    for (relative, absolute) in files {
        generated.push_str(&format!(
            "    ({relative:?}, include_bytes!({absolute:?})),\n",
            absolute = absolute.to_string_lossy()
        ));
    }
    generated.push_str("];\n");
    fs::write(output, generated).expect("write embedded web manifest");
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()));
    for entry in entries {
        let entry = entry.expect("read web export entry");
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("web asset is under export root")
                .to_string_lossy()
                .replace('\\', "/");
            files.push((relative, path));
        }
    }
}
