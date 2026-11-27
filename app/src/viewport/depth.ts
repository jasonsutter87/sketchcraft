// Depth offsets for the faces and the pictures drawn over them. A textured face is drawn twice, on the same plane: the
// base mesh (its colour) and the picture over it. Which one wins is decided by polygon offset alone, and a perspective
// camera's depth buffer is far coarser at distance than a parallel one's (it is not linear: with the near plane at 5 cm
// and the far plane at 20 km, one step is half a millimetre at 20 m and centimetres at 100 m). An offset of one unit
// (factor 1, units 1) is within that coarseness, so as the view orbited, bits of the base showed through the picture
// (white, "partly mapped"), and only in perspective (#222). Pictures sit well in front of the base they cover.
export interface Offset {
  factor: number;
  units: number;
}

/** Plain faces: a little behind edges and lines, which are drawn at depth 0. */
export const FACE_OFFSET: Offset = { factor: 1, units: 1 };
/** The base mesh when pictures are drawn over it: pushed back enough to stay behind them at any distance. */
export const FACE_OFFSET_UNDER_PICTURES: Offset = { factor: 2, units: 16 };
/** A picture over its base: at the depth of edges and lines (it does not cover them). */
export const PICTURE_OFFSET: Offset = { factor: 0, units: 0 };

/** The depth steps by which the picture is in front of the base under it (more is steadier). */
export function pictureLead(base: Offset = FACE_OFFSET_UNDER_PICTURES, picture: Offset = PICTURE_OFFSET): number {
  return base.units - picture.units;
}
