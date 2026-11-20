import importlib.util
import io
import json
import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile

spec=importlib.util.spec_from_file_location('universal',Path(__file__).parents[1]/'tools/android_universal.py')
universal=importlib.util.module_from_spec(spec);spec.loader.exec_module(universal)
class UniversalTests(unittest.TestCase):
    def engine(self, reverse=False, value=1, broken_hash=False, lambda_type='Block'):
        files={
            'render-definitions.json': json.dumps({'blocks':{'stone':value}}, indent=2 if reverse else None).encode(),
            'creative-inventory.json': json.dumps({'tabs':[{'items':['stone','wood'],'tags':[['b','a'] if reverse else ['a','b']]}]}).encode(),
            'block-behaviours.json': json.dumps({'blocks':[{'fields':{'shapes':f'<{lambda_type}$$Lambda/0x{456 if reverse else 123:x}>'}}]}).encode(),
            'registry.bin':b'fixed registry IDs',
        }
        files['horizon-resources.json']=json.dumps({'schema':7,'render_files_sha256':{
            'render-definitions.json':'0'*64 if broken_hash else hashlib.sha256(files['render-definitions.json']).hexdigest()
        }}).encode()
        buf=io.BytesIO()
        with zipfile.ZipFile(buf,'w',compression=zipfile.ZIP_DEFLATED if reverse else zipfile.ZIP_STORED) as z:
            for name,data in sorted(files.items(),reverse=reverse):z.writestr(name,data)
        return buf.getvalue()

    def fixture(self, root):
        inputs=[]
        for abi,(machine,elfclass) in universal.MACHINES.items():
            path=root/(abi+'.apk')
            elf=bytearray(32);elf[:6]=b'\x7fELF'+bytes([elfclass,1]);elf[18:20]=machine.to_bytes(2,'little')
            with zipfile.ZipFile(path,'w') as z:
                for name in ['AndroidManifest.xml','classes.dex','resources.arsc','assets/engine-data.zip']:
                    z.writestr(name,self.engine() if name == 'assets/engine-data.zip' else b'identical shared payload')
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
                self.assertEqual(z.getinfo('resources.arsc').compress_type,zipfile.ZIP_STORED)
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

    def test_exporter_order_and_runtime_addresses_do_not_change_engine_semantics(self):
        self.assertEqual(universal.engine_contents(self.engine()),universal.engine_contents(self.engine(reverse=True)))
        self.assertNotEqual(universal.engine_contents(self.engine()),universal.engine_contents(self.engine(value=2)))
        self.assertNotEqual(universal.engine_contents(self.engine()),universal.engine_contents(self.engine(lambda_type='OtherBlock')))

    def test_modified_engine_file_with_stale_hash_is_rejected(self):
        with self.assertRaisesRegex(ValueError,'checksum mismatch'):
            universal.engine_contents(self.engine(broken_hash=True))

    def test_universal_retains_original_verified_pack_but_rejects_gameplay_changes(self):
        for changed in [False,True]:
            with tempfile.TemporaryDirectory() as d:
                root=Path(d);inputs=self.fixture(root)
                with zipfile.ZipFile(inputs[1]) as z:data={n:z.read(n) for n in z.namelist()}
                data['assets/engine-data.zip']=self.engine(reverse=True,value=2 if changed else 1)
                with zipfile.ZipFile(inputs[1],'w') as z:
                    for name,value in data.items():z.writestr(name,value)
                out=root/'out.apk'
                if changed:
                    with self.assertRaisesRegex(ValueError,'semantically'):universal.assemble(inputs,out)
                else:
                    universal.assemble(inputs,out)
                    with zipfile.ZipFile(out) as z:self.assertEqual(z.read('assets/engine-data.zip'),self.engine())
