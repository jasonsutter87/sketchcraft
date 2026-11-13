# Photoreal renderer (built in)

A path tracer in `core/`, no Blender or plugins: works in the app, headless
and over MCP.

## R1: core path tracer
- Triangles from the model (components and textures included), a BVH (SAH, binned)
- Physically based materials: diffuse, rough / smooth specular (GGX), metal, glass, emission
- Sun (soft, with angular size) + sky dome; next-event estimation toward the sun
- Unidirectional path tracing, Russian roulette, ~6 bounces
- Multithreaded tiles; progressive (samples accumulate)

## R2: clean images fast
- Albedo + normal buffers; an edge-aware à-trous denoiser guided by them
- Filmic (ACES) tone mapping, exposure

## R3: in the app
- Photoreal render window that refines while you watch; Preview / Final
- Sun from the model's location + time of day; render saved views in a batch

## R4: materials
- Roughness, metalness, glass, glow per material; textures
- Smart defaults: names with glass / window are glass, metal / steel / chrome are metal

## R5: quality
- Depth of field, portals for interiors, bloom, white balance
