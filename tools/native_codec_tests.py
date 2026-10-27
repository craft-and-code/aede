"""Offline native-codec provenance and public-API security regressions."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
AUDIO = ROOT / "crates/aede-audio"


class NativeCodecTests(unittest.TestCase):
    def test_retained_vendor_sources_match_the_recorded_upstream_and_patch_hashes(self):
        vendor = AUDIO / "vendor"
        originals = json.loads((vendor / "sha256.json").read_text())
        patched = json.loads((vendor / "patches/sha256.json").read_text())
        self.assertEqual(set(patched), {"speex-1.2.1/libspeex/bits.c", "musepack_src_r475/libmpcdec/mpc_bits_reader.h", "musepack_src_r475/libmpcdec/mpc_demux.c"})
        for name, digest in originals.items():
            with self.subTest(file=name):
                self.assertEqual(hashlib.sha256((vendor / name).read_bytes()).hexdigest(), patched.get(name, digest))
        recorded = {name for name in originals if name.endswith((".c", ".h"))}
        retained = {p.relative_to(vendor).as_posix() for p in vendor.rglob("*") if p.suffix in (".c", ".h")}
        self.assertEqual(retained, recorded)

    def test_encoded_and_reference_fixture_bytes_are_unchanged(self):
        fixtures = AUDIO / "tests/fixtures/native-extra"
        for name, digest in json.loads((fixtures / "sha256.json").read_text()).items():
            with self.subTest(file=name):
                self.assertEqual(hashlib.sha256((fixtures / name).read_bytes()).hexdigest(), digest)

    def compiler(self):
        if os.name == "nt":
            self.skipTest("Standalone POSIX cc probe; provenance and native Rust codec fixtures cover Windows separately")
        compiler = shutil.which("cc") or shutil.which("clang") or shutil.which("gcc")
        if not compiler:
            self.skipTest("Standalone C sentinel needs a cc-compatible compiler; Rust codec fixtures remain independent")
        return compiler

    def test_speex_security_backport_preserves_the_non_owned_buffer_sentinel(self):
        compiler = self.compiler()
        speex = AUDIO / "vendor/speex-1.2.1"
        original_hash = json.loads((AUDIO / "vendor/sha256.json").read_text())["speex-1.2.1/libspeex/bits.c"]
        # Reverse only the upstream one-line fix to prove that this sentinel
        # actually detects the historical defect, not merely process startup.
        fixed = (speex / "libspeex/bits.c").read_bytes()
        start = fixed.index(b"EXPORT void speex_bits_read_whole_bytes")
        marker = b"nchars=bits->buf_size-(bits->nbBits>>LOG2_BITS_PER_CHAR)-1;"
        tail = fixed[start:]
        at = tail.index(b'"Do not own input buffer: truncating oversize input"')
        original = fixed[:start] + tail[:at] + tail[at:].replace(marker, b"nchars=bits->buf_size;", 1)
        self.assertEqual(hashlib.sha256(original).hexdigest(), original_hash)
        with tempfile.TemporaryDirectory(prefix="aede-codec-bits-") as folder:
            folder = Path(folder)
            for name, source, expected in [("original", original, 1), ("patched", fixed, 0)]:
                code = folder / (name + ".c")
                code.write_bytes(source)
                binary = folder / name
                command = [compiler, "-std=c11", "-DFLOATING_POINT=1", "-DEXPORT=", "-I" + str(speex / "include"), "-I" + str(speex / "libspeex"), str(AUDIO / "tests/native-c/speex-bits.c"), str(code), "-o", str(binary)]
                built = subprocess.run(command, cwd=folder, capture_output=True, text=True)
                self.assertEqual(built.returncode, 0, built.stderr)
                result = subprocess.run([str(binary)], cwd=folder, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stderr)

    def test_musepack_word_assembly_uses_defined_unsigned_c_shifts(self):
        compiler = self.compiler()
        mpc = AUDIO / "vendor/musepack_src_r475"
        with tempfile.TemporaryDirectory(prefix="aede-codec-word-") as folder:
            folder = Path(folder)
            binary = folder / "word"
            command = [compiler, "-std=c11", "-O1", "-fsanitize=undefined", "-fno-sanitize-recover=all", "-I" + str(mpc / "include"), "-I" + str(mpc / "libmpcdec"), str(AUDIO / "tests/native-c/musepack-bits.c"), "-o", str(binary)]
            built = subprocess.run(command, cwd=folder, capture_output=True, text=True)
            self.assertEqual(built.returncode, 0, built.stderr)
            result = subprocess.run([str(binary)], cwd=folder, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_musepack_enumerative_masks_include_bit_31_without_signed_overflow(self):
        compiler = self.compiler()
        mpc = AUDIO / "vendor/musepack_src_r475"
        with tempfile.TemporaryDirectory(prefix="aede-codec-enum-") as folder:
            folder = Path(folder)
            binary = folder / "enum"
            command = [compiler, "-std=c11", "-O1", "-fsanitize=undefined", "-fno-sanitize-recover=all", "-I" + str(mpc / "include"), "-I" + str(mpc / "libmpcdec"), str(AUDIO / "tests/native-c/musepack-enum.c"), str(mpc / "libmpcdec/mpc_bits_reader.c"), "-o", str(binary)]
            built = subprocess.run(command, cwd=folder, capture_output=True, text=True)
            self.assertEqual(built.returncode, 0, built.stderr)
            result = subprocess.run([str(binary)], cwd=folder, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_musepack_count_compatibility_uses_standard_integer_operations(self):
        compiler = self.compiler()
        with tempfile.TemporaryDirectory(prefix="aede-codec-count-") as folder:
            folder = Path(folder)
            source = folder / "counts.c"
            source.write_text('#include <mpc/minimax.h>\nint main(void) { return !(mini(0, 9) == 0 && mini(UINT32_MAX, 7) == 7 && mini(UINT32_MAX, UINT32_MAX) == UINT32_MAX && absi(-17) == 17 && absi(0) == 0); }\n')
            binary = folder / "counts"
            built = subprocess.run([compiler, "-std=c11", "-I" + str(AUDIO / "native"), str(source), "-o", str(binary)], cwd=folder, capture_output=True, text=True)
            self.assertEqual(built.returncode, 0, built.stderr)
            self.assertEqual(subprocess.run([str(binary)], cwd=folder).returncode, 0)

    def test_musepack_alignment_and_signed_seek_deltas_use_defined_arithmetic(self):
        compiler = self.compiler()
        mpc = AUDIO / "vendor/musepack_src_r475"
        sources = [mpc / "libmpcdec" / (name + ".c") for name in ["huffman", "mpc_decoder", "requant", "streaminfo", "synth_filter", "mpc_bits_reader"]]
        sources += [mpc / "common" / name for name in ["crc32.c", "huffman-bcl.c"]]
        with tempfile.TemporaryDirectory(prefix="aede-codec-demux-") as folder:
            folder = Path(folder)
            binary = folder / "demux"
            command = [compiler, "-std=c11", "-O1", "-fsanitize=undefined", "-fno-sanitize-recover=all", "-DLONG_SEEK_TABLE=1", "-I" + str(AUDIO / "native"), "-I" + str(mpc / "include"), "-I" + str(mpc / "libmpcdec"), str(AUDIO / "tests/native-c/musepack-demux.c"), *map(str, sources), "-o", str(binary), "-lm"]
            built = subprocess.run(command, cwd=folder, capture_output=True, text=True)
            self.assertEqual(built.returncode, 0, built.stderr)
            for mode in ["fill", "seek", "negative", "positive", "wide"]:
                with self.subTest(mode=mode):
                    result = subprocess.run([str(binary), mode], cwd=folder, capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr)

    def test_corrupt_wavpack_reaches_crc_failure_with_defined_predictor_arithmetic(self):
        compiler = self.compiler()
        wavpack = AUDIO / "vendor/wavpack-5.9.0"
        # Exercise the production WavPack source group and its compiler flags,
        # so removing its arithmetic contract also breaks this sanitizer case.
        build = (AUDIO / "build.rs").read_text()
        group = re.search(r'for name in \[(.*?)\] \{\s*wavpack_build.file', build, re.S)
        self.assertIsNotNone(group)
        sources = [wavpack / "src" / (name + ".c") for name in re.findall(r'"([a-zA-Z0-9_]+)"', group[1])]
        flags = re.findall(r'wavpack_build.flag_if_supported\("([^"]+)"\)', build)
        with tempfile.TemporaryDirectory(prefix="aede-codec-wavpack-") as folder:
            folder = Path(folder)
            binary = folder / "wavpack"
            command = [compiler, "-std=c11", "-O1", "-fsanitize=undefined", "-fno-sanitize-recover=all", *flags, "-I" + str(AUDIO / "native"), "-I" + str(wavpack / "include"), str(AUDIO / "tests/native-c/wavpack-corrupt.c"), str(AUDIO / "native/wavpack.c"), *map(str, sources), "-o", str(binary), "-lm"]
            built = subprocess.run(command, cwd=folder, capture_output=True, text=True)
            self.assertEqual(built.returncode, 0, built.stderr)
            original = (AUDIO / "tests/fixtures/native-extra/track.wv").read_bytes()
            for offset, value in [(36, 0), (52, 0), (55, 255)]:
                with self.subTest(offset=offset):
                    data = bytearray(original)
                    data[offset] = value
                    path = folder / "corrupt.wv"
                    path.write_bytes(data)
                    result = subprocess.run([str(binary), str(path)], cwd=folder, capture_output=True, text=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
