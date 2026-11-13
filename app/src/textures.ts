// Turning an image file into a texture: downscale to at most 1024 px (models
// stay small), keep PNGs with transparency as PNG, find the average color
// (renders and the engine use it).

export interface Prepared {
  data: string;
  color: [number, number, number];
  aspect: number; // height / width
  width: number;
  height: number;
}

export async function prepareImage(file: File, max = 1024): Promise<Prepared> {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((ok, bad) => {
      const i = new Image();
      i.onload = () => ok(i);
      i.onerror = () => bad(new Error("That file isn't an image the browser can read"));
      i.src = url;
    });
    const scale = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
    const w = Math.max(1, Math.round(img.naturalWidth * scale));
    const h = Math.max(1, Math.round(img.naturalHeight * scale));
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const g = c.getContext("2d")!;
    g.drawImage(img, 0, 0, w, h);
    const px = g.getImageData(0, 0, w, h).data;
    let r = 0, gg = 0, b = 0, n = 0, alpha = false;
    for (let i = 0; i < px.length; i += 16) {
      r += px[i];
      gg += px[i + 1];
      b += px[i + 2];
      n++;
      if (px[i + 3] < 250) alpha = true;
    }
    const data = alpha ? c.toDataURL("image/png") : c.toDataURL("image/jpeg", 0.86);
    return { data, color: [r / n / 255, gg / n / 255, b / n / 255], aspect: h / w, width: w, height: h };
  } finally {
    URL.revokeObjectURL(url);
  }
}
