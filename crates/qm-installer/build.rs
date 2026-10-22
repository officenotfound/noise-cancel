//! Packs the folder named by QM_PAYLOAD_DIR (the packaged app, see scripts/package_windows.sh) into a zip that the
//! installer embeds. Without it the installer still builds (for UI work) and reports an empty payload.
use std::io::Write;
use std::path::Path;

fn add(zw: &mut zip::ZipWriter<std::fs::File>, root: &Path, dir: &Path) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            add(zw, root, &p);
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            // models are mostly compressed already
            let method = if rel.ends_with(".gz") { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
            zw.start_file(rel, zip::write::SimpleFileOptions::default().compression_method(method)).unwrap();
            zw.write_all(&std::fs::read(&p).unwrap()).unwrap();
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=QM_PAYLOAD_DIR");
    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join("payload.zip");
    let mut zw = zip::ZipWriter::new(std::fs::File::create(&out).unwrap());
    if let Some(dir) = std::env::var_os("QM_PAYLOAD_DIR") {
        // relative paths are taken from the workspace root, not from this crate's folder
        let root = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
        let dir = root.join(dir);
        add(&mut zw, &dir, &dir);
    }
    zw.finish().unwrap();
}
