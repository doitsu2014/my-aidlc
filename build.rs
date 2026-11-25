use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("read methodology assets")
        .map(Result::unwrap)
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type().expect("read asset type");
        if kind.is_dir() {
            collect(root, &path, out);
        } else if kind.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-changed=core/data/phase-graph.json");
    println!("cargo:rerun-if-changed=core/methodology");
    let root = Path::new("core");
    let mut files = vec![(
        "data/phase-graph.json".to_owned(),
        root.join("data/phase-graph.json"),
    )];
    collect(root, &root.join("methodology"), &mut files);
    let mut generated = String::from("pub static ASSETS: &[(&str, &[u8])] = &[\n");
    for (rel, path) in files {
        let absolute = fs::canonicalize(path).expect("asset must exist");
        generated.push_str(&format!(
            "({rel:?}, include_bytes!({:?})),\n",
            absolute.to_string_lossy()
        ));
    }
    generated.push_str("];\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("embedded_assets.rs"),
        generated,
    )
    .unwrap();
}
