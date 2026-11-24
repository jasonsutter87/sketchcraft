# 2. Drawing

## The drawing tools

| Tool | Key | How |
| --- | --- | --- |
| Line | L | click, click, click; close a loop and it becomes a face |
| Rectangle | R | two corners. Type `5,4` for an exact size |
| Rotated Rectangle | | one side, then the other |
| Circle | C | center, then radius. Type a number of sides, like `24s` |
| Polygon | N | center, then a corner |
| 2-Point Arc | A | the two ends, then the bulge |
| Arc (center) / 3-Point Arc / Pie | | from the tool bar or Shift+S |
| Freehand | | drag to draw; close it for a face |
| Curve | U | click points; a smooth curve goes through all of them (below) |

### Curves through points

The **Curve** tool (**U**, also on the Draw menu) draws a smooth spline through the points you click. Click the first
point, keep clicking, then **double-click** or press **Enter** to finish. Click back on the first point to close the
curve; if the points are flat, a closed curve makes a face. You see the curve as you go, and what you see is what
you get. Esc drops it.

Type a number and Enter to set how many pieces each stretch between two points is cut into (the default is 12; `24` is
smoother, `4` is rougher). The setting stays for the next curve. The result is a chain of edges that is picked,
offset and followed as one curve, like an arc. One Undo takes it back.

Agents can draw the same thing with `spline {points, closed?, segments?}`, or draw cubic Bezier curves with
`bezier {points, closed?, segments?}`, where the points are anchor, handle, handle, anchor (then handle, handle,
anchor for each next piece).

Lines that cross a face split it; lines that meet make faces on their own.
That's how you cut a door outline into a wall: draw it on the wall, then
push it in.

## Exact sizes: just type

While a tool is running, type the size and press Enter. It goes in the
**Value** box at the bottom right.

- Lengths in any unit: `2.5`, `250cm`, `8'6"`, `12'`, `90mm`. The model's
  units are under **Document › Units**.
- Rectangles: `width,depth`.
- Copies (after a Move with Option/Ctrl): `x5` makes five more in a row, `/5`
  fills the gap with five.

## Snapping (inference)

SketchCraft shows what the cursor is locked to, with a colored dot and a label:

| Dot | Means |
| --- | --- |
| green | **Endpoint** of an edge |
| green ring | **Center** of an arc or circle |
| blue | **Midpoint** of an edge |
| yellow square | the **Origin** |
| red | **On Edge** |
| blue diamond | **On Face** |
| a red, green or blue line | along an **axis** |

Hover a point, then move away: a dotted line shows when you're lined up with
it ("From Point"). Hover two points and move to where their lines cross: both
dotted lines show and the cursor lands on the crossing ("From Points"). That
finds the middle of a board or a floor: hover the midpoints of two sides that
meet, then move toward the middle. Press an **arrow key** to lock an axis:
→ red, ← green, ↑ blue. **Shift** holds whatever you're locked to.

## Guides and measuring

- **Tape Measure** (T): measures, and clicking an edge then pulling out leaves
  a guide line. Type a distance for an exact offset.
- **Protractor**: an angled guide.
- **Dimension** (D): click, click, pull out. **Text** (W): a label with a leader.

Next: [Shaping](03-shaping.md)
