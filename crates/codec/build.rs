//! On Linux, build the FFmpeg decoder shim (src/ffmpeg/shim.c) when the
//! system has libavcodec's headers (pkg-config), and turn on `sunna_ffmpeg`.
//! Without them the viewer still works, decoding H.264 with OpenH264.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(sunna_ffmpeg)");
    println!("cargo::rerun-if-changed=src/ffmpeg/shim.c");
    println!("cargo::rerun-if-env-changed=SUNNA_NO_FFMPEG");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") || std::env::var_os("SUNNA_NO_FFMPEG").is_some() {
        return;
    }
    // libavcodec 58 is FFmpeg 4.x (Ubuntu 22.04, Debian 11); libavutil
    // numbers its versions differently.
    let libs = [("libavcodec", "58"), ("libavutil", "56")].map(|(name, version)| pkg_config::Config::new().atleast_version(version).probe(name));
    if let Some(error) = libs.iter().find_map(|lib| lib.as_ref().err()) {
        println!("cargo::warning=FFmpeg development files not found ({error}); the Linux viewer decodes H.264 only, in software. Install libavcodec-dev and libavutil-dev (Debian/Ubuntu) or ffmpeg (Arch) for hardware and HEVC decoding.");
        return;
    }
    let mut build = cc::Build::new();
    build.file("src/ffmpeg/shim.c").warnings(true);
    for lib in libs.iter().flatten() {
        for path in &lib.include_paths {
            build.include(path);
        }
    }
    build.compile("sunna_ffmpeg_shim");
    println!("cargo::rustc-cfg=sunna_ffmpeg");
}
