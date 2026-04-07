#!/usr/bin/env node
// Converts PNG frames to GIF using gif-encoder-2 + canvas
// Usage: node assets/frames-to-gif.mjs

import GIFEncoder from 'gif-encoder-2';
import { createCanvas, loadImage } from 'canvas';
import { createWriteStream, readdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const framesDir = join(__dirname, 'frames');
const outputGif = join(__dirname, 'demo-twitter.gif');

const WIDTH = 1200;
const HEIGHT = 675;
const FPS = 12;

async function main() {
  const files = readdirSync(framesDir)
    .filter(f => f.endsWith('.png'))
    .sort();

  console.log(`Processing ${files.length} frames...`);

  // Use octree algorithm for better quality
  const encoder = new GIFEncoder(WIDTH, HEIGHT, 'octree', false);
  const stream = createWriteStream(outputGif);

  encoder.createReadStream().pipe(stream);
  encoder.start();
  encoder.setDelay(1000 / FPS);
  encoder.setRepeat(0); // Loop forever

  const canvas = createCanvas(WIDTH, HEIGHT);
  const ctx = canvas.getContext('2d');

  for (let i = 0; i < files.length; i++) {
    const img = await loadImage(join(framesDir, files[i]));
    ctx.drawImage(img, 0, 0, WIDTH, HEIGHT);
    encoder.addFrame(ctx);

    if (i % 20 === 0) process.stdout.write(`\r  Frame ${i}/${files.length}`);
  }

  encoder.finish();
  console.log(`\n✓ GIF saved to ${outputGif}`);

  await new Promise(resolve => stream.on('finish', resolve));
}

main().catch(console.error);
