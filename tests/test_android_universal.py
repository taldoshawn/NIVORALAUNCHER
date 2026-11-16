import importlib.util
from pathlib import Path
import tempfile
import unittest
import zipfile

spec=importlib.util.spec_from_file_location('universal',Path(__file__).parents[1]/'tools/android_universal.py')
universal=importlib.util.module_from_spec(spec);spec.loader.exec_module(universal)
class UniversalTests(unittest.TestCase):
    def fixture(self, root):
        inputs=[]
        for abi,(machine,elfclass) in universal.MACHINES.items():
            path=root/(abi+'.apk')
            elf=bytearray(32);elf[:6]=b'\x7fELF'+bytes([elfclass,1]);elf[18:20]=machine.to_bytes(2,'little')
            with zipfile.ZipFile(path,'w') as z:
                for name in ['AndroidManifest.xml','classes.dex','resources.arsc','assets/engine-data.zip']:
                    z.writestr(name,b'identical shared payload')
                z.writestr('lib/'+abi+'/libhorizon_android.so',elf)
                z.writestr('META-INF/OLD.RSA',b'old signature')
            inputs.append(path)
        return inputs
    def test_contains_all_libraries_and_one_shared_payload_without_old_signatures(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);inputs=self.fixture(root);out=root/'universal.apk'
            universal.assemble(inputs,out)
            with zipfile.ZipFile(out) as z:
                self.assertEqual(len([n for n in z.namelist() if n.startswith('lib/')]),4)
                self.assertEqual(z.read('classes.dex'),b'identical shared payload')
                self.assertNotIn('META-INF/OLD.RSA',z.namelist())
    def test_conflicting_shared_resources_are_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);inputs=self.fixture(root)
            # Rewrite without duplicates to check the actual shared-payload guard.
            with zipfile.ZipFile(inputs[0]) as z:data={n:z.read(n) for n in z.namelist()}
            data['classes.dex']=b'different code'
            with zipfile.ZipFile(inputs[0],'w') as z:
                for name,value in data.items():z.writestr(name,value)
            with self.assertRaisesRegex(ValueError,'Shared APK'):universal.assemble(inputs,root/'out.apk')
            self.assertFalse((root/'out.apk').exists())
    def test_missing_or_duplicate_architectures_are_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);inputs=self.fixture(root)
            with self.assertRaises(ValueError):universal.assemble(inputs[:3],root/'out.apk')
            with self.assertRaises(ValueError):universal.assemble([inputs[0]]*4,root/'out.apk')

    def test_wrong_elf_architecture_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);inputs=self.fixture(root)
            with zipfile.ZipFile(inputs[0]) as z:data={n:z.read(n) for n in z.namelist()}
            name=next(n for n in data if n.startswith('lib/'))
            bad=bytearray(data[name]);bad[18:20]=(62).to_bytes(2,'little');data[name]=bad
            with zipfile.ZipFile(inputs[0],'w') as z:
                for name,value in data.items():z.writestr(name,value)
            with self.assertRaisesRegex(ValueError,'architecture mismatch'):universal.assemble(inputs,root/'out.apk')
