
fn main() {
    tonic_build::configure()
        .out_dir("src")
        .compile(&["src/point.proto"], &["proto/"]).unwrap();
}