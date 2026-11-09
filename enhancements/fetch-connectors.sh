#!/bin/sh
set -eu
task_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 - "$task_root/enhancements/connector-catalog.json" "$@" <<'PY'
from pathlib import Path
import hashlib,json,os,subprocess,sys,tempfile,zipfile
catalogue=json.loads(Path(sys.argv[1]).read_text())
default=Path(os.environ.get('XDG_DATA_HOME',str(Path.home()/'.local/share')))/'solidcraft/connectors'
library=Path(sys.argv[2] if len(sys.argv)>2 else os.environ.get('SOLIDCRAFT_CONNECTOR_LIBRARY',str(default)))
library.mkdir(parents=True,exist_ok=True)
for model in catalogue:
    target=library/model['filename']
    if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest()==model['step_sha256']:
        print('Verified:',model['part_number']);continue
    with tempfile.TemporaryDirectory(prefix='solidcraft-connector-') as folder:
        archive=Path(folder)/'model.zip'
        subprocess.run(['curl','--fail','--location','--retry','2','--max-time','60','--silent','--show-error','--output',str(archive),model['archive_url']],check=True)
        if hashlib.sha256(archive.read_bytes()).hexdigest()!=model['archive_sha256']:
            raise RuntimeError('Manufacturer archive changed: '+model['part_number'])
        with zipfile.ZipFile(archive) as bundle:
            entry=bundle.getinfo(model['step_member'])
            if entry.file_size>10_000_000:raise RuntimeError('Unexpectedly large CAD model')
            data=bundle.read(entry)
        if hashlib.sha256(data).hexdigest()!=model['step_sha256']:
            raise RuntimeError('CAD checksum mismatch')
        next_file=target.with_suffix('.next');next_file.write_bytes(data);os.replace(next_file,target)
        print('Installed:',model['part_number'])
(library/'catalogue.json').write_text(json.dumps(catalogue,indent=2)+'\n')
PY
