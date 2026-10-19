import json

inv = json.load(open("artifacts/visualize/inv-src.json", encoding="utf-8"))
print("== GROUPS ==")
print(json.dumps(inv["groups"], indent=2))
print("\n== EDGES (internal import graph, %d edges) ==" % len(inv["edges"]))
for e in inv["edges"]:
    frm = e['from'].replace('kernelledger.', 'kl.')
    to = e['to'].replace('kernelledger.', 'kl.')
    print(f"  {frm}  ->  {to}")

ext = {}
for m in inv["modules"]:
    for x in m["imports_external"]:
        ext[x] = ext.get(x, 0) + 1
lazy = inv.get("dashed", [])
print("\n== LAZY (in-function) INTERNAL IMPORTS (dashed, %d) ==" % len(lazy))
for e in lazy:
    print(f"  {e['from'].replace('kernelledger.', 'kl.')}  --  {e['to'].replace('kernelledger.', 'kl.')}")
print("\n== EXTERNAL IMPORTS (third-party roots) ==")
print(json.dumps(ext, indent=2))
print("\n== OPTIONAL MODULES (import torch/numpy/pandas/pyarrow) ==")
for m in inv["modules"]:
    if m["optional"]:
        print(f"  {m['name']}  external={m['imports_external']}")
