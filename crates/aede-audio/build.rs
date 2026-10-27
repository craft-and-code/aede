fn main() {
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed=vendor");
    #[cfg(feature = "decode")]
    {
        compile_codecs();
        compile_legacy_ape();
    }
}

#[cfg(feature = "decode")]
fn compile_codecs() {
    let root = std::path::Path::new("vendor");
    let wavpack = root.join("wavpack-5.9.0");
    let speex = root.join("speex-1.2.1");
    let musepack = root.join("musepack_src_r475");
    let mut c = cc::Build::new();
    c.std("c11")
        .warnings(false)
        .include(wavpack.join("include"))
        .include(speex.join("include"))
        .include(musepack.join("include"))
        .include("native")
        .define("FLOATING_POINT", "1")
        .define("EXPORT", "")
        .define("SPEEX_VERSION", "\"1.2.1\"")
        .define("SPEEX_MAJOR_VERSION", "1")
        .define("SPEEX_MINOR_VERSION", "2")
        .define("SPEEX_MICRO_VERSION", "1")
        .define("SPEEX_EXTRA_VERSION", "\"\"")
        .define("LONG_SEEK_TABLE", "1")
        .define("_CRT_SECURE_NO_WARNINGS", "1");
    // WavPack's integer predictor arithmetic needs modulo signed add/multiply,
    // including while rejecting corrupt packets. Keep that compiler contract
    // confined to WavPack; other codecs retain ordinary overflow diagnostics.
    let mut wavpack_build = c.clone();
    wavpack_build.flag_if_supported("-fwrapv");
    for name in [
        "common_utils",
        "decorr_utils",
        "entropy_utils",
        "extra1",
        "extra2",
        "open_utils",
        "open_filename",
        "open_legacy",
        "open_raw",
        "pack",
        "pack_dns",
        "pack_floats",
        "pack_utils",
        "read_words",
        "tags",
        "tag_utils",
        "unpack",
        "unpack_floats",
        "unpack_seek",
        "unpack_utils",
        "write_words",
    ] {
        wavpack_build.file(wavpack.join("src").join(format!("{name}.c")));
    }
    wavpack_build.file("native/wavpack.c");
    wavpack_build.compile("aede_wavpack");
    for name in [
        "cb_search",
        "exc_10_32_table",
        "exc_8_128_table",
        "filters",
        "gain_table",
        "hexc_table",
        "high_lsp_tables",
        "lsp",
        "ltp",
        "speex",
        "stereo",
        "vbr",
        "vq",
        "bits",
        "exc_10_16_table",
        "exc_20_32_table",
        "exc_5_256_table",
        "exc_5_64_table",
        "gain_table_lbr",
        "hexc_10_32_table",
        "lpc",
        "lsp_tables_nb",
        "modes",
        "modes_wb",
        "nb_celp",
        "quant_lsp",
        "sb_celp",
        "speex_callbacks",
        "speex_header",
        "window",
    ] {
        c.file(speex.join("libspeex").join(format!("{name}.c")));
    }
    for name in [
        "huffman",
        "mpc_decoder",
        "requant",
        "streaminfo",
        "synth_filter",
        "mpc_bits_reader",
        "mpc_demux",
    ] {
        c.file(musepack.join("libmpcdec").join(format!("{name}.c")));
    }
    c.file(musepack.join("common/crc32.c"));
    c.file(musepack.join("common/huffman-bcl.c"));
    c.file("native/musepack.c");
    c.file("native/speex.c");
    // Generic C paths, no optional assembly, codec threads, DSP or downloads.
    c.compile("aede_native_codecs");
    if !std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|s| s == "windows") {
        println!("cargo:rustc-link-lib=m");
    }
}

// Build the approved upstream SDK without its console/UI targets. Keep each
// optional SIMD unit's flags local; dispatch remains the SDK's responsibility.
#[cfg(feature = "decode")]
fn compile_legacy_ape() {
    let root = std::path::Path::new("vendor/monkeys-audio-13.43/Source");
    let mut cpp = cc::Build::new();
    cpp.cpp(true)
        .std("c++11")
        .opt_level(2)
        .warnings(false)
        .flag_if_supported("-fwrapv")
        .include(root.join("Shared"))
        .include(root.join("MACLib"))
        .include("native");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let platform = match target_os.as_str() {
        "windows" => "PLATFORM_WINDOWS",
        "macos" | "ios" => "PLATFORM_APPLE",
        "android" => "PLATFORM_ANDROID",
        _ => "PLATFORM_LINUX",
    };
    if std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|v| v == "musl") {
        cpp.cpp_link_stdlib_static(true);
    }
    cpp.define(platform, None)
        .define("_CRT_SECURE_NO_WARNINGS", None);
    if target_os == "windows" {
        cpp.flag_if_supported("/EHsc");
    }
    for directory in ["Shared", "MACLib", "MACLib/Old"] {
        let mut paths: Vec<_> = std::fs::read_dir(root.join(directory))
            .expect("vendored SDK directory")
            .map(|entry| entry.expect("vendored SDK file").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
            .collect();
        paths.sort();
        for path in paths {
            let name = path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default();
            if name.starts_with("NNFilter")
                && name != "NNFilter.cpp"
                && name != "NNFilterGeneric.cpp"
            {
                continue;
            }
            if (name == "WinFileIO.cpp" && target_os != "windows")
                || (name == "StdLibFileIO.cpp" && target_os == "windows")
            {
                continue;
            }
            cpp.file(path);
        }
    }
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let units: &[(&str, &[&str])] = match arch.as_str() {
        "x86" | "x86_64" => &[
            ("SSE2", &["-msse2"]),
            ("SSE4.1", &["-msse4.1"]),
            ("AVX2", &["-mavx2"]),
            ("AVX512", &["-mavx512dq", "-mavx512bw"]),
        ],
        "arm" | "aarch64" => &[("Neon", &[])],
        "riscv32" | "riscv64" => &[("RVV", &[])],
        "powerpc" | "powerpc64" => &[("AltiVec", &[])],
        _ => &[],
    };
    for (unit, flags) in units {
        // Compile only this unit; the shared builder's files belong to the
        // final archive and must not be compiled again with SIMD-only flags.
        let mut build = cc::Build::new();
        build
            .cpp(true)
            .std("c++11")
            .opt_level(2)
            .warnings(false)
            .flag_if_supported("-fwrapv")
            .include(root.join("Shared"))
            .include(root.join("MACLib"))
            .define(platform, None)
            .define("_CRT_SECURE_NO_WARNINGS", None)
            .file(root.join("MACLib").join(format!("NNFilter{unit}.cpp")));
        if target_os == "windows" {
            build.flag_if_supported("/EHsc");
            if *unit == "AVX2" {
                build.flag_if_supported("/arch:AVX2");
            }
            if *unit == "AVX512" {
                build.flag_if_supported("/arch:AVX512");
            }
        }
        for flag in *flags {
            build.flag_if_supported(flag);
        }
        cpp.objects(build.compile_intermediates());
    }
    cpp.file("native/ape.cpp").compile("aede_ape_legacy");
}
