#!/bin/sh
set -eu
task_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 - "$task_root/enhancements/connector-catalog.json" "$@" <<'PY'
from pathlib import Path
from urllib.parse import urlparse,quote
from concurrent.futures import ThreadPoolExecutor
import hashlib,json,os,subprocess,sys,tempfile,threading,zipfile
catalogue=json.loads(Path(sys.argv[1]).read_text())
default=Path(os.environ.get('XDG_DATA_HOME',str(Path.home()/'.local/share')))/'solidcraft/connectors'
library=Path(sys.argv[2] if len(sys.argv)>2 else os.environ.get('SOLIDCRAFT_CONNECTOR_LIBRARY',str(default)))
library.mkdir(parents=True,exist_ok=True)
archives=library/'archives';archives.mkdir(exist_ok=True)
locks={m['archive_sha256']:threading.Lock() for m in catalogue if m['filename']}
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def install(model):
    if not model['filename']:return 'Reference only: '+model['label']
    target=library/model['filename']
    if target.exists() and digest(target)==model['step_sha256']:return 'Verified: '+model['part_number']
    suffix=Path(urlparse(model['archive_url']).path).suffix.lower()
    archive=archives/(model['archive_sha256']+suffix)
    with locks[model['archive_sha256']]:
        if not archive.exists() or digest(archive)!=model['archive_sha256']:
            with tempfile.TemporaryDirectory(prefix='solidcraft-connector-') as folder:
                download=Path(folder)/('source'+suffix)
                subprocess.run(['curl','--fail','--location','--retry','2','--max-time','60','--max-filesize','50000000','--silent','--show-error','--output',str(download),quote(model['archive_url'],safe=':/?&=%')],check=True)
                if digest(download)!=model['archive_sha256']:raise RuntimeError('Manufacturer archive changed: '+model['part_number'])
                next_archive=archive.with_suffix(archive.suffix+'.next');next_archive.write_bytes(download.read_bytes());os.replace(next_archive,archive)
        if model['step_member']:
            with zipfile.ZipFile(archive) as bundle:
                entry=bundle.getinfo(model['step_member'])
                if entry.file_size>20_000_000:raise RuntimeError('Unexpectedly large CAD model')
                data=bundle.read(entry)
        else:data=archive.read_bytes()
        if len(data)>20_000_000 or hashlib.sha256(data).hexdigest()!=model['step_sha256']:raise RuntimeError('CAD checksum mismatch')
        with tempfile.NamedTemporaryFile(prefix='connector-',suffix='.next',dir=library,delete=False) as temporary:
            temporary.write(data);next_file=Path(temporary.name)
        try:os.replace(next_file,target)
        finally:next_file.unlink(missing_ok=True)
    return 'Installed: '+model['part_number']
# Models sharing the same archive are serialised by its hash; unrelated downloads run together.
with ThreadPoolExecutor(max_workers=4) as pool:
    for status in pool.map(install,catalogue):print(status,flush=True)
(library/'catalogue.json').write_text(json.dumps(catalogue,ensure_ascii=False,indent=2)+'\n')
PY
