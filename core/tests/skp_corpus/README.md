# SketchUp test models

A few small `.skp` files from SketchUp's own test suite,
[SketchUp/testup-2](https://github.com/SketchUp/testup-2) (commit
`8d9c2acdb0d4a46f5e2cddc60a2fc4a0f67a09bd`), which is MIT licensed:
Copyright (c) 2014-2024 Trimble Inc. The license is in [LICENSE](LICENSE).
SketchCraft is not affiliated with or endorsed by Trimble or SketchUp.

They are real files from SketchUp itself, one per format era, so `skp_corpus.rs`
can check that our clean-room reader keeps reading what the real program wrote:

| File here | testup-2 file | Saved by | Format |
| --------- | ------------- | -------- | ------ |
| `v06-extruded-box.skp` | `Sketchup__skp-v6.skp` | SketchUp 6 | old |
| `v06-screen-text.skp` | `Sketchup_DefinitionList__screentext.skp` | SketchUp 6 | old; one screen text, no geometry (#59) |
| `v13-extruded-circle.skp` | `Sketchup_ArcCurve__ExtrudedCircle.skp` | 2013 | 2013-2016 |
| `v20-model.skp` | `Sketchup_Skp__2020.skp` | 2020 | 2017-2020 |
| `v21-components.skp` | `Sketchup_ComponentDefinition__2021_lc.skp` | 2021 | 2021-2022 |
| `v22-soft-block.skp` | `Sketchup__skp-vff-soft-block.skp` | 2022 | 2021-2022, new block format |

Adding one: copy it here under a name that starts with the version, add its row
above and its counts to `CORPUS` in `skp_corpus.rs`. Keep the whole folder under 3 MB.
