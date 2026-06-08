const fs = require("fs");
const path = require("path");
const sharp = require("sharp");

const IN_DIR = path.join(__dirname, "..", "public");
const OUT_DIR = path.join(IN_DIR, "icons");

const variants = ["icon.svg", "icon-light.svg"];
const sizes = [48, 72, 96, 144, 192, 256, 384, 512];

const splashSizes = [
  { name: "apple-splash-1125-2436", w: 1125, h: 2436 },
  { name: "apple-splash-1242-2688", w: 1242, h: 2688 },
  { name: "apple-splash-1536-2048", w: 1536, h: 2048 },
  { name: "apple-splash-1668-2388", w: 1668, h: 2388 },
  { name: "apple-splash-2048-2732", w: 2048, h: 2732 },
];

async function generate() {
  for (const src of variants) {
    const svg = fs.readFileSync(path.join(IN_DIR, src), "utf-8");
    const base = path.basename(src, ".svg");
    const suffix = base === "icon-light" ? "light" : null;

    for (const size of sizes) {
      const name = suffix ? `icon-${size}x${size}-${suffix}.png` : `icon-${size}x${size}.png`;

      const regular = sharp(Buffer.from(svg)).resize(size, size).png();

      await regular.toFile(path.join(OUT_DIR, name));
      console.log(`Generated ${name}`);

      const safeZone = Math.round(size * 0.8);
      const padding = Math.round((size - safeZone) / 2);
      const maskableName = suffix
        ? `icon-${size}x${size}-maskable-${suffix}.png`
        : `icon-${size}x${size}-maskable.png`;

      const icon = await sharp(Buffer.from(svg)).resize(safeZone, safeZone).png().toBuffer();

      await sharp({
        create: {
          width: size,
          height: size,
          channels: 4,
          background: { r: 0, g: 0, b: 0, alpha: 0 },
        },
      })
        .png()
        .composite([{ input: icon, top: padding, left: padding }])
        .toFile(path.join(OUT_DIR, maskableName));
      console.log(`Generated ${maskableName}`);
    }
  }

  const [primarySvg] = variants;
  const primaryBuf = fs.readFileSync(path.join(IN_DIR, primarySvg), "utf-8");
  const appleIcon = sharp(Buffer.from(primaryBuf)).resize(180, 180).png();
  await appleIcon.toFile(path.join(OUT_DIR, "apple-touch-icon.png"));
  console.log("Generated apple-touch-icon.png");

  const splashBg =
    '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1" fill="#0f172a"/></svg>';
  const splashIconSvg = fs.readFileSync(path.join(IN_DIR, "icon-light.svg"), "utf-8");

  for (const { name, w, h } of splashSizes) {
    const iconSize = Math.round(Math.min(w, h) * 0.25);
    const iconBuf = await sharp(Buffer.from(splashIconSvg))
      .resize(iconSize, iconSize)
      .png()
      .toBuffer();
    const left = Math.round((w - iconSize) / 2);
    const top = Math.round((h - iconSize) / 2);
    await sharp(Buffer.from(splashBg))
      .resize(w, h)
      .png()
      .composite([{ input: iconBuf, top, left }])
      .toFile(path.join(OUT_DIR, `${name}.png`));
    console.log(`Generated ${name}.png`);
  }
}

generate().catch((err) => {
  console.error(err);
  process.exit(1);
});
