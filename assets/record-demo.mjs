#!/usr/bin/env node
// Records demo-animation.html frame-by-frame with clock control
// Usage: node assets/record-demo.mjs

import puppeteer from 'puppeteer';
import { readdirSync, unlinkSync, existsSync, mkdirSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const framesDir = join(__dirname, 'frames');
const htmlPath = `file://${join(__dirname, 'demo-animation.html').replace(/\\/g, '/')}`;

const FPS = 12;
const FRAME_MS = 1000 / FPS;
const DURATION_MS = 20500;
const TOTAL_FRAMES = Math.ceil(DURATION_MS / FRAME_MS);

async function main() {
  if (existsSync(framesDir)) {
    readdirSync(framesDir).forEach(f => unlinkSync(join(framesDir, f)));
  } else {
    mkdirSync(framesDir, { recursive: true });
  }

  console.log('Launching browser...');
  const browser = await puppeteer.launch({
    headless: true,
    args: ['--no-sandbox'],
  });

  const page = await browser.newPage();
  await page.setViewport({ width: 1200, height: 675, deviceScaleFactor: 1 });

  // Intercept page clock — we control time
  await page.evaluateOnNewDocument(() => {
    let fakeNow = 0;
    const origPerf = performance.now.bind(performance);
    performance.now = () => fakeNow;
    window.__setFakeTime = (t) => { fakeNow = t; };
    // Also override Date.now for good measure
    const origDate = Date.now;
    Date.now = () => fakeNow;
  });

  console.log(`Opening ${htmlPath}`);
  await page.goto(htmlPath, { waitUntil: 'domcontentloaded' });

  // Wait for page to initialize
  await new Promise(r => setTimeout(r, 200));

  console.log(`Recording ${TOTAL_FRAMES} frames at ${FPS}fps (${(DURATION_MS/1000).toFixed(1)}s)...`);

  for (let i = 0; i < TOTAL_FRAMES; i++) {
    const t = i * FRAME_MS;

    // Set fake time and trigger animation frame
    await page.evaluate((time) => {
      window.__setFakeTime(time);
      // Manually trigger rAF callback chain
    }, t);

    // Give the page a moment to process
    await page.evaluate(() => new Promise(r => requestAnimationFrame(r)));
    await page.evaluate(() => new Promise(r => requestAnimationFrame(r)));

    const filename = join(framesDir, `frame_${String(i).padStart(4, '0')}.png`);
    await page.screenshot({ path: filename, type: 'png' });

    if (i % 20 === 0) process.stdout.write(`\r  Frame ${i}/${TOTAL_FRAMES}`);
  }

  console.log(`\nDone. ${TOTAL_FRAMES} frames in ${framesDir}`);
  await browser.close();
}

main().catch(console.error);
