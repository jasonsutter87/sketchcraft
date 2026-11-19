# Texturing test set

Six small models that every 0.3.9 texturing feature is built and tested on: the beaver plus dice, can, globe, crate and
soccer ball. They are ordinary built-in assets (like the fixtures), so a test loads one with
`{"cmd":"insert","asset":"dice","at":[0,0,0]}` in the CLI, a script, a Rust test or the bridge, and a person finds them in
the Library under **Texturing test set**. All geometry is original and MIT, made by our own code.

## The models

| Key | Size (m) | Built from | Tests |
| --- | --- | --- | --- |
| `beaver` | 0.66 × 0.01 × 1.5 | the existing flat figure (`brand/beaver.py`), faces the camera | planar projection and painting on an uneven outline; Position Texture |
| `dice` | 0.05 × 0.05 × 0.05 | one closed cube, 6 square faces, one material (Ivory plastic) | box projection, per-face texture placement; pips are texture-only later |
| `can` | 0.066 × 0.066 × 0.122 | closed 32-sided cylinder: 32 side faces (soft edges, one curved surface) and 2 caps (Aluminium) | cylindrical projection, label wrap, seam position |
| `globe` | 0.30 × 0.30 × 0.35 | `Sphere` (16 segments × 10 rings, 160 faces, triangles at the poles), `Base` and `Stem` as separate groups | spherical projection, pole pinching, UV seams |
| `crate` | 0.60 × 0.60 × 0.60 | group `Crate` of 34 closed boxes: 4 posts, 4 sides × 5 planks, floor and lid × 5 boards (Pine, Pine dark) | paint-through across connected faces (it must stop at a box edge, or cross it when asked), planar projection onto many coplanar faces |
| `soccer_ball` | 0.22 across, rests on the floor | one closed solid: 12 pentagons (Ball black) and 20 hexagons (Ball white), 60 vertices, 90 edges | per-face paint, the UV unwrap stress test (every face is its own island), spherical projection |

Origin: on the floor at the middle of the model (the beaver stands between its feet). Faces point outward and every
solid is closed (`volume` returns it); the can, globe and ball are intentionally faceted so projections have edges to
land on. Each carries a default material (see the table) so tests can check material assignment survives an operation.

## Where things live

- `tools/texturing_set/texturing_set.py` writes `core/assets/{dice,can,globe,crate,soccer_ball}.jsonl`
  (`--check` fails when a file is stale). Pure Python; edit it and regenerate, never the `.jsonl` by hand.
- `core/src/assets.rs`: the five registrations (`about` carries the size). The beaver was already registered.
- `app/src/texturing-set.ts`: the Library list (key and size); `app/src/library.tsx`: the "Texturing test set" collection,
  searchable by model name or by the collection name; `locale-en.ts` / `locale-it.ts`: `texset.*` keys.
- Tests: `core/tests/texturing_set.rs` (face counts, closed solids, sizes, outward normals, export),
  `app/tests/texturing-set.test.ts`, `tools/ui-tests/texturing-set.mjs`.

## Using it for a new feature

Add the feature's test to the model it is for (a Rust test builds it with `sketchcraft::assets::build`, or inserts it into an
`Engine`), and add a Playwright step that places the model from the Library. Do not add a new model unless none of the six
has the topology you need; if you do, add it to `texturing_set.py`, `assets.rs`, `texturing-set.ts`, the locale keys and the
tests above.

## Known limits

- No visual reference image yet: add one with the first feature that changes how these look (macOS references can only be
  generated on a Mac, see `tools/ui-tests/visual.mjs`).
- The dice have no pips (they are meant to be texture-only), and the can and globe have no printed art; both come with the
  texturing features.
- The soccer ball is flat-faced, not inflated; the globe's poles are triangle fans.
