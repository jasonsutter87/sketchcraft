"""Check a SketchCraft IFC export with IfcOpenShell: schema and EXPRESS
rules, and that every element's geometry can be built.

    python3 -m venv /tmp/ifcenv && /tmp/ifcenv/bin/pip install ifcopenshell pytest
    /tmp/ifcenv/bin/python tools/ifc/check_ifc.py model.ifc

Exits 1 when there are validation issues or geometry failures.
"""
import sys, ifcopenshell, ifcopenshell.validate, ifcopenshell.geom
path = sys.argv[1]
f = ifcopenshell.open(path)
print("schema", f.schema, "entities", len(list(f)))
log = ifcopenshell.validate.json_logger()
ifcopenshell.validate.validate(f, log, express_rules=True)
print("validation issues:", len(log.statements))
for s in log.statements[:15]:
    print(" -", {k: (str(v)[:160]) for k, v in s.items() if k in ("type","message","attribute","instance")})
st = ifcopenshell.geom.settings()
ok = bad = 0
errs = []
for p in f.by_type("IfcProduct"):
    if not p.Representation:
        continue
    try:
        sh = ifcopenshell.geom.create_shape(st, p)
        ok += 1
    except Exception as e:
        bad += 1
        errs.append((p.is_a(), p.GlobalId, str(e)[:100]))
print("geometry ok", ok, "failed", bad, errs[:5])
print("types", sorted({p.is_a() for p in f.by_type("IfcProduct")}))
print("storeys", [s.Name for s in f.by_type("IfcBuildingStorey")], "units", [u.Name for u in f.by_type("IfcSIUnit")])

sys.exit(1 if log.statements or bad else 0)
