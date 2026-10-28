"""Offline guards for the native-library qualification, without compiling Apps."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[1]

def module(name):
    spec = importlib.util.spec_from_file_location('native_platform_' + name, ROOT / 'tools/native-platform' / (name + '.py'))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

class NativePlatformTests(unittest.TestCase):
    def test_prepare_creates_only_test_targets_and_keeps_an_existing_frozen_lock(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            aede, fc, ph, output = [root / p for p in ['aede', 'flaccompagnon', 'phemios', 'acceptance']]
            aede.mkdir(); (aede / 'Cargo.lock').write_text('reviewed lock\n')
            prepare = module('prepare')
            prepare.prepare(aede, fc, ph, output)
            self.assertEqual((output / 'Cargo.lock').read_text(), 'reviewed lock\n')
            (output / 'Cargo.lock').write_text('frozen test lock\n')
            prepare.prepare(aede, fc, ph, output)
            self.assertEqual((output / 'Cargo.lock').read_text(), 'frozen test lock\n')
            text = (output / 'Cargo.toml').read_text()
            self.assertIn('default-features = false', text)
            self.assertIn('ape-pcm16', text)
            self.assertNotIn('tauri', text)
            self.assertTrue((output / 'tests/acceptance.rs').is_file())
    def test_cached_reference_with_wrong_hash_is_refused_without_overwriting_it(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); (root / 'sh3.ape').write_bytes(b'wrong')
            with patch('urllib.request.urlopen', side_effect=AssertionError('unexpected network')):
                with self.assertRaises(ValueError): module('prepare').reference(root)
            self.assertEqual((root / 'sh3.ape').read_bytes(), b'wrong')
    def test_pe_architecture_is_checked_for_both_windows_targets(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'acceptance.exe'
            for architecture, machine in [('x86_64', 0x8664), ('aarch64', 0xaa64)]:
                data = bytearray(128); data[:2] = b'MZ';struct.pack_into('<I', data, 0x3c, 64)
                data[64:68] = b'PE\0\0';struct.pack_into('<H', data, 68, machine);binary.write_bytes(data)
                audit = module('run').audit
                self.assertEqual(audit(binary, architecture + '-pc-windows-msvc')['machine'], machine)
                other = 'aarch64' if architecture == 'x86_64' else 'x86_64'
                with self.assertRaises(ValueError):audit(binary, other + '-pc-windows-msvc')
    def test_musl_requires_native_machine_and_no_dynamic_dependencies(self):
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'acceptance';binary.write_bytes(b'ELF')
            run = module('run')
            class Output:
                def __init__(self, text):self.stdout = text
            for segments, dynamic, good in [('no loader', 'no dynamic section', True), ('INTERP', '', False), ('', '(NEEDED)', False)]:
                with patch.object(run, 'execute', side_effect=[Output('Machine: AArch64'), Output(segments), Output(dynamic)]):
                    if good:self.assertTrue(run.audit(binary, 'aarch64-unknown-linux-musl')['static'])
                    else:
                        with self.assertRaises(ValueError):run.audit(binary, 'aarch64-unknown-linux-musl')
    def test_local_runner_cannot_build_product_apps(self):
        text = (ROOT / 'tools/native-platform/run.py').read_text()
        for command in ['cargo build', 'npm run build', 'prepare-engine', 'build:native', 'tools/check.sh']:
            self.assertNotIn(command, text)
        self.assertIn('"aede-audio"', text)
        self.assertIn('"acceptance"', text)

if __name__ == '__main__': unittest.main()
