#!/usr/bin/env node
//
// The recipe screens, measured at the widths people actually hold.
//
// A screenshot proves nothing on its own — a reviewer looks at one page in
// one theme and the next change breaks the other three. So this drives a real
// browser over the recipe list, the read view, the editor and the diary at
// 390x844 and 360x800, in light and dark, and asserts what "fits on a phone"
// means: the page never scrolls sideways, no row overflows the card it is in,
// nothing overlaps its neighbour, and anything you have to tap is big enough
// to hit with a thumb.
//
// It builds its own account and its own deliberately awkward recipe — a long
// name, a long brand, a sub-recipe, an ingredient that is only words, a
// five-step method, a photo, and every nutrient switched on — because the
// layouts only break on the content nobody uses in a demo.
//
// It also covers the amount control: a count beside a unit is two more things
// competing for the same 280 pixels, and the weight it works out is the
// figure people came for. So a food with a household portion is logged by the
// portion, and the dialog, the diary row and the editor row are all measured
// with it in them.
//
// Usage:
//   node scripts/mobile-layout.mjs [--api URL] [--web URL] [--shots DIR]
//
// Needs an API and a built or running frontend, and Playwright's Chromium:
//   npm i -g playwright && npx playwright install chromium
//   PLAYWRIGHT_CHROMIUM=/path/to/chrome  # to point at one you already have

import { deflateSync } from 'node:zlib'
import { mkdirSync } from 'node:fs'
import { createRequire } from 'node:module'

const args = Object.fromEntries(
  process.argv
    .slice(2)
    .join(' ')
    .matchAll(/--(\w+)[= ]([^\s]+)/g)
    .map((m) => [m[1], m[2]]),
)
const API = (args.api ?? process.env.API ?? 'http://127.0.0.1:8117') + '/api/v1'
const WEB = args.web ?? process.env.WEB ?? 'http://127.0.0.1:5217'
const SHOTS = args.shots ?? process.env.SHOTS ?? 'mobile-shots'

let failures = 0
const pass = (label) => process.stdout.write(`  \u001b[32m✓\u001b[0m ${label}\n`)
const fail = (label, detail) => {
  failures++
  process.stdout.write(`  \u001b[31m✗\u001b[0m ${label}${detail ? ` — ${detail}` : ''}\n`)
}
const check = (ok, label, detail) => (ok ? pass(label) : fail(label, detail))

// ---------------------------------------------------------------- the fixture

async function call(path, { method = 'GET', body, token } = {}) {
  const headers = {}
  if (token) headers.authorization = `Bearer ${token}`
  if (body) headers['content-type'] = 'application/json'
  const res = await fetch(`${API}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status} ${await res.text()}`)
  return res.status === 204 ? null : res.json()
}

/** A real PNG, made here rather than checked in: a gradient, w by h. */
function png(w, h) {
  const raw = Buffer.alloc((w * 3 + 1) * h)
  let at = 0
  for (let y = 0; y < h; y++) {
    raw[at++] = 0
    for (let x = 0; x < w; x++) {
      raw[at++] = Math.round(40 + (180 * x) / w)
      raw[at++] = Math.round(90 + (120 * y) / h)
      raw[at++] = Math.round(70 + (60 * (x + y)) / (w + h))
    }
  }
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data])
    const len = Buffer.alloc(4)
    len.writeUInt32BE(data.length)
    const crcTable = png.crcTable ?? (png.crcTable = buildCrcTable())
    let crc = 0xffffffff
    for (const b of body) crc = crcTable[(crc ^ b) & 0xff] ^ (crc >>> 8)
    const tail = Buffer.alloc(4)
    tail.writeUInt32BE((crc ^ 0xffffffff) >>> 0)
    return Buffer.concat([len, body, tail])
  }
  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(w, 0)
  ihdr.writeUInt32BE(h, 4)
  ihdr[8] = 8
  ihdr[9] = 2
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ])
}

function buildCrcTable() {
  const table = new Int32Array(256)
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    table[n] = c
  }
  return table
}

async function fixture() {
  const stamp = `${Date.now()}-${Math.floor(Math.random() * 1e4)}`
  const auth = await call('/auth/register', {
    method: 'POST',
    body: {
      email: `mobile-${stamp}@example.test`,
      password: 'mobile-layout-password',
      display_name: 'Alexandra Featherstonehaugh',
    },
  })
  const token = auth.access_token

  // Every nutrient on: eight plus net carbs is the worst case a macro row
  // ever has to wrap, and a user can ask for exactly that in Settings.
  await call('/profile', {
    method: 'PATCH',
    token,
    body: {
      tracking_focus: 'custom',
      shown_nutrients: [
        'calories_kcal',
        'protein_g',
        'carbs_g',
        'net_carbs_g',
        'fat_g',
        'fiber_g',
        'sugar_g',
        'saturated_fat_g',
        'sodium_mg',
      ],
    },
  })
  await call('/targets', {
    method: 'PUT',
    token,
    body: {
      targets: [
        { nutrient: 'calories_kcal', kind: 'budget', amount: 2200 },
        { nutrient: 'protein_g', kind: 'goal', amount: 140 },
        { nutrient: 'sodium_mg', kind: 'budget', amount: 2300 },
      ],
    },
  })

  const food = (f) =>
    call('/foods', { method: 'POST', token, body: { nutrient_basis: 'per_100g', ...f } })

  const oats = await food({
    name: 'Organic wholegrain rolled oats, old fashioned',
    brand: "Bob's Red Mill Whole Grain Foods of Oregon",
    serving_size_g: 40,
    serving_label: '1/2 cup',
    calories_kcal: 379,
    protein_g: 13.2,
    carbs_g: 67.7,
    fat_g: 6.5,
    fiber_g: 10.1,
    sugar_g: 0.9,
    saturated_fat_g: 1.2,
    sodium_mg: 6,
  })
  const yoghurt = await food({
    name: 'Greek style natural yoghurt, five percent fat',
    brand: 'Fage Total Authentic Strained Yoghurt',
    serving_size_g: 170,
    calories_kcal: 97,
    protein_g: 9,
    carbs_g: 3.8,
    fat_g: 5,
    fiber_g: 0,
    sugar_g: 3.8,
    saturated_fat_g: 3.3,
    sodium_mg: 36,
  })
  const honey = await food({
    name: 'Honey',
    serving_size_g: 21,
    calories_kcal: 304,
    protein_g: 0.3,
    carbs_g: 82.4,
    fat_g: 0,
    fiber_g: 0.2,
    sugar_g: 82.1,
    saturated_fat_g: 0,
    sodium_mg: 4,
  })
  const walnuts = await food({
    name: 'Walnut halves and pieces, raw and unsalted',
    brand: 'Sunnyside Orchards Premium Californian',
    serving_size_g: 30,
    calories_kcal: 654,
    protein_g: 15.2,
    carbs_g: 13.7,
    fat_g: 65.2,
    fiber_g: 6.7,
    sugar_g: 2.6,
    saturated_fat_g: 6.1,
    sodium_mg: 2,
  })
  const butter = await food({
    name: 'Salted butter',
    brand: 'Kerrygold Pure Irish',
    serving_size_g: 10,
    calories_kcal: 717,
    protein_g: 0.9,
    carbs_g: 0.1,
    fat_g: 81,
    fiber_g: 0,
    sugar_g: 0.1,
    saturated_fat_g: 51,
    sodium_mg: 643,
  })

  const compote = await call('/recipes', {
    method: 'POST',
    token,
    body: {
      name: 'Slow spiced winter fruit compote',
      description: 'Keeps a week in the fridge.',
      servings: 6,
      items: [
        { food_id: honey.id, quantity_g: 60 },
        { food_id: walnuts.id, quantity_g: 45 },
        { label: 'a cinnamon stick' },
      ],
    },
  })

  const recipe = await call('/recipes', {
    method: 'POST',
    token,
    body: {
      name: 'Overnight oats with spiced winter compote and toasted walnuts',
      description:
        'A make-ahead breakfast that keeps three days in the fridge and needs nothing in the morning but a spoon.\nScale it by the jar: one jar is one serving, and the compote is made once for the week.',
      instructions: [
        'Tip the oats into a jar with a lid that actually seals, and pour the yoghurt over them.',
        'Stir until every oat is wet — dry pockets stay dry overnight and they are unpleasant.',
        'Spoon the compote on top, seal the jar and leave it in the fridge for at least eight hours.',
        'Toast the walnuts in a dry pan until they smell of walnuts rather than of nothing, about four minutes.',
        'In the morning, stir once, scatter the walnuts over and eat it out of the jar.',
      ].join('\n'),
      servings: 3,
      is_public: true,
      items: [
        { food_id: oats.id, quantity_g: 240 },
        { food_id: yoghurt.id, quantity_g: 500 },
        { sub_recipe_id: compote.id, servings: 1.5 },
        { food_id: butter.id, quantity_g: 12 },
        { label: 'a good pinch of flaky sea salt, and more honey to taste' },
      ],
    },
  })

  const form = new FormData()
  form.append('file', new Blob([png(900, 600)], { type: 'image/png' }), 'jar.png')
  form.append('caption', 'The jar, the morning after')
  const shot = await fetch(`${API}/recipes/${recipe.id}/photos`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}` },
    body: form,
  })
  if (!shot.ok) throw new Error(`photo -> ${shot.status} ${await shot.text()}`)

  // The food the whole feature is for: nobody weighs a chicken breast, they
  // count them. Two of these is 348 g, and both halves of that have to fit.
  const chicken = await food({
    name: 'Chicken breast fillet, skinless and boneless, raw',
    brand: 'Ballymaloe Free Range Poultry Company',
    serving_size_g: 100,
    calories_kcal: 120,
    protein_g: 22.5,
    carbs_g: 0,
    fat_g: 2.6,
    fiber_g: 0,
    sugar_g: 0,
    saturated_fat_g: 0.8,
    sodium_mg: 63,
  })
  const withPortion = await call(`/foods/${chicken.id}/portions`, {
    method: 'POST',
    token,
    body: { label: 'chicken breast', grams: 174 },
  })
  const breast = withPortion.portions.find((p) => p.label === 'chicken breast')

  const now = new Date()
  const date = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}-${String(now.getDate()).padStart(2, '0')}`
  for (const entry of [
    { meal: 'breakfast', recipe_id: recipe.id, recipe_servings: 1 },
    { meal: 'breakfast', food_id: oats.id, quantity_g: 55 },
    { meal: 'lunch', food_id: yoghurt.id, quantity_g: 200 },
  ]) {
    await call('/diary', { method: 'POST', token, body: { logged_on: date, ...entry } })
  }

  // Two of them, said as two of them. A server that does not take amounts
  // that way gets the weight instead, and the row is then asserted to say
  // the weight — the phrase is whatever the server sends back either way,
  // which is the whole point: the client never composes it.
  try {
    await call('/diary', {
      method: 'POST',
      token,
      body: {
        logged_on: date,
        meal: 'dinner',
        food_id: chicken.id,
        portion_id: breast.id,
        portion_count: 2,
      },
    })
  } catch {
    await call('/diary', {
      method: 'POST',
      token,
      body: { logged_on: date, meal: 'dinner', food_id: chicken.id, quantity_g: 348 },
    })
  }

  // A food with nothing to count it in, for the path that adds one. Named so
  // no seeded measure matches it: the whole point is a food that has none.
  const burrito = await food({
    name: 'Chicken breast burrito, shop bought and frozen',
    serving_size_g: 220,
    calories_kcal: 198,
    protein_g: 9.4,
    carbs_g: 24.1,
    fat_g: 7.2,
  })
  await call('/diary', {
    method: 'POST',
    token,
    body: { logged_on: date, meal: 'snack', food_id: burrito.id, quantity_g: 220 },
  })

  // What the row should read, taken from the server rather than written out
  // here — an `amount_label` is the server's to word, and a suite that spells
  // it out itself would be asserting its own opinion of the phrasing.
  const day = await call(`/diary/day?date=${date}`, { token })
  const logged = day.meals
    .flatMap((m) => m.entries)
    .find((e) => e.food_id === chicken.id)
  const said =
    logged.portion_count != null
      ? `${logged.amount_label} · ${Math.round(logged.quantity_g)} g`
      : logged.amount_label

  return {
    token,
    recipeId: recipe.id,
    chicken: chicken.name,
    burrito: burrito.name,
    said,
  }
}

// ------------------------------------------------------------- the assertions

async function noSideScroll(page, label) {
  const over = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  )
  check(over <= 1, `${label}: no horizontal scroll`, `${over}px of overflow`)
}

/** Nothing inside a box may stick out of that box's own width. */
async function within(page, label, selector, what = 'card') {
  const spills = await page.evaluate((sel) => {
    const bad = []
    for (const card of document.querySelectorAll(sel)) {
      const c = card.getBoundingClientRect()
      for (const el of card.querySelectorAll('*')) {
        if (el.children.length) continue
        // A progress bar's fill is translated out of its own track on
        // purpose, and clipped there; it is never visibly outside anything.
        if (el.closest('[data-slot="progress"]')) continue
        const r = el.getBoundingClientRect()
        if (r.width === 0 && r.height === 0) continue
        if (r.right > c.right + 1 || r.left < c.left - 1) {
          bad.push(`${el.tagName.toLowerCase()} "${(el.textContent || '').trim().slice(0, 30)}"`)
        }
      }
    }
    return bad
  }, selector)
  check(
    spills.length === 0,
    `${label}: nothing spills out of its ${what}`,
    spills.slice(0, 3).join('; '),
  )
}

/** Every card on the page, which is where this app puts everything. */
const withinCards = (page, label) => within(page, label, '[data-slot="card"]')

/** Siblings in a list row must not sit on top of each other. */
async function noOverlap(page, label, selector) {
  const clashes = await page.evaluate((sel) => {
    const bad = []
    for (const row of document.querySelectorAll(sel)) {
      const kids = [...row.children].map((el) => ({
        el,
        r: el.getBoundingClientRect(),
      }))
      for (let i = 0; i < kids.length; i++) {
        for (let j = i + 1; j < kids.length; j++) {
          const a = kids[i].r
          const b = kids[j].r
          if (a.width === 0 || b.width === 0) continue
          const overX = Math.min(a.right, b.right) - Math.max(a.left, b.left)
          const overY = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top)
          if (overX > 1 && overY > 1) {
            bad.push(
              `"${(kids[i].el.textContent || '').trim().slice(0, 20)}" over "${(kids[j].el.textContent || '').trim().slice(0, 20)}"`,
            )
          }
        }
      }
    }
    return bad
  }, selector)
  check(clashes.length === 0, `${label}: row parts do not overlap`, clashes.slice(0, 3).join('; '))
}

/** A control you have to hit with a thumb. 44px is the number everyone uses. */
async function tappable(page, label, selector, min = 40) {
  const small = await page.evaluate(
    ([sel, m]) => {
      const bad = []
      for (const el of document.querySelectorAll(sel)) {
        const r = el.getBoundingClientRect()
        if (r.width === 0 && r.height === 0) continue
        if (getComputedStyle(el).visibility === 'hidden') continue
        if (r.height < m || r.width < m) {
          bad.push(
            `${(el.getAttribute('aria-label') || el.textContent || el.tagName).trim().slice(0, 28)} ${Math.round(r.width)}x${Math.round(r.height)}`,
          )
        }
      }
      return bad
    },
    [selector, min],
  )
  check(small.length === 0, `${label}: controls are thumb-sized`, small.slice(0, 4).join('; '))
}

/** Text that has been squeezed until it cannot show a word. */
async function readable(page, label, selector, min = 90) {
  const narrow = await page.evaluate(
    ([sel, m]) => {
      const bad = []
      for (const el of document.querySelectorAll(sel)) {
        const r = el.getBoundingClientRect()
        if (r.width > 0 && r.width < m) bad.push(`"${(el.textContent || '').trim().slice(0, 24)}" ${Math.round(r.width)}px`)
      }
      return bad
    },
    [selector, min],
  )
  check(narrow.length === 0, `${label}: names have room to read`, narrow.slice(0, 3).join('; '))
}

// ------------------------------------------------------------------- the pass

const require = createRequire(import.meta.url)
let chromium
for (const spec of ['playwright', 'playwright-core', '/opt/node22/lib/node_modules/playwright']) {
  try {
    ;({ chromium } = require(spec))
    break
  } catch {
    /* try the next one */
  }
}
if (!chromium) {
  console.error('Playwright is not installed. npm i -g playwright && npx playwright install chromium')
  process.exit(2)
}

const VIEWPORTS = [
  { name: 'iphone', width: 390, height: 844 },
  { name: 'android', width: 360, height: 800 },
  { name: 'desktop', width: 1280, height: 900 },
]
const THEMES = ['light', 'dark']

const { token, recipeId, chicken, burrito, said } = await fixture()
mkdirSync(SHOTS, { recursive: true })

const browser = await chromium.launch({
  executablePath: process.env.PLAYWRIGHT_CHROMIUM || undefined,
})

for (const viewport of VIEWPORTS) {
  const phone = viewport.name !== 'desktop'
  for (const theme of THEMES) {
    const context = await browser.newContext({
      viewport: { width: viewport.width, height: viewport.height },
      deviceScaleFactor: 2,
      hasTouch: phone,
      isMobile: phone,
      colorScheme: theme,
    })
    await context.addInitScript(
      ([t, th]) => {
        localStorage.setItem('nom-inal.token', t)
        localStorage.setItem('nom-inal.theme', th)
      },
      [token, theme],
    )

    const tag = `${viewport.name}-${theme}`
    console.log(`\n== ${viewport.width}x${viewport.height}, ${theme}`)

    /**
     * One screen, on a page of its own.
     *
     * A full-page screenshot resizes the viewport under the page and Chromium
     * does not put the touch emulation back afterwards, so every measurement
     * taken after the first screenshot would quietly be a measurement of the
     * desktop. A page per screen keeps the picture and the assertions from
     * interfering, and costs one navigation.
     */
    const scene = async (name, body, fullPage = true) => {
      const page = await context.newPage()
      await body(page)
      // Dialogs and menus fade and zoom in; a picture taken on arrival is a
      // picture of a half-transparent one.
      await page.waitForTimeout(400)
      await page.screenshot({ path: `${SHOTS}/${tag}-${name}.png`, fullPage })
      await page.close()
    }

    /** Touch emulation reaches the page after it loads; nothing is measured until it has. */
    const touchReady = (page) =>
      phone
        ? page.waitForFunction(() => matchMedia('(pointer: coarse)').matches, null, {
            timeout: 5000,
          })
        : Promise.resolve()

    // The list.
    await scene('list', async (page) => {
      await page.goto(`${WEB}/recipes`, { waitUntil: 'networkidle' })
      await touchReady(page)
      await page.waitForSelector('text=Overnight oats')
      await noSideScroll(page, 'recipes list')
      await withinCards(page, 'recipes list')
      if (phone) await tappable(page, 'recipes list', '[aria-label^="Delete "]')
    })

    // The read view.
    await scene('read', async (page) => {
      await page.goto(`${WEB}/recipes/${recipeId}`, { waitUntil: 'networkidle' })
      await touchReady(page)
      await page.waitForSelector('h1')
      await page.waitForSelector('.print-photos img')
      await noSideScroll(page, 'read view')
      await withinCards(page, 'read view')
      await noOverlap(page, 'read view', 'ul.divide-y > li')
      await readable(page, 'read view', '[data-ingredient-name]', phone ? 120 : 200)
      if (!phone) return
      await tappable(page, 'read view photo delete', '[aria-label="Delete photo"]', 32)
      await tappable(page, 'read view nav', 'header nav a', 30)
      const title = await page.locator('h1').first().boundingBox()
      check(
        title.y < viewport.height / 3,
        'read view: the recipe starts near the top',
        `${Math.round(title.y)}px down`,
      )
    })

    // The actions that do not fit a phone, behind one button.
    if (phone) {
      await scene(
        'menu',
        async (page) => {
          await page.goto(`${WEB}/recipes/${recipeId}`, { waitUntil: 'networkidle' })
          await touchReady(page)
          const trigger = page.locator('[aria-label="More recipe actions"]')
          check((await trigger.count()) === 1, 'read view: one overflow menu, always visible')
          await trigger.click()
          await page.waitForSelector('[data-slot="dropdown-menu-content"]')
          await page.waitForTimeout(350)
          const menu = await page.locator('[data-slot="dropdown-menu-content"]').boundingBox()
          check(
            menu.x >= 0 && menu.x + menu.width <= viewport.width,
            'read view: the overflow menu opens on screen',
            `${Math.round(menu.x)}..${Math.round(menu.x + menu.width)}`,
          )
          await tappable(page, 'read view menu', '[data-slot="dropdown-menu-item"]', 38)
        },
        false,
      )
    }

    // The editor.
    await scene('editor', async (page) => {
      await page.goto(`${WEB}/recipes/${recipeId}`, { waitUntil: 'networkidle' })
      await touchReady(page)
      await page.getByRole('button', { name: 'Edit' }).first().click()
      await page.waitForSelector('#r-name')
      await noSideScroll(page, 'editor')
      await withinCards(page, 'editor')
      await noOverlap(page, 'editor', 'ul.divide-y > li')
      check(
        (await page.locator('#r-name').boundingBox()).width >= (phone ? 140 : 300),
        'editor: the name box is wide enough to type in',
      )
      const amount = await page.locator('#ingredient-0-count').boundingBox()
      check(
        amount.width >= 56 && amount.height >= (phone ? 40 : 32),
        'editor: the count box fits a number and a thumb',
        `${Math.round(amount.width)}x${Math.round(amount.height)}`,
      )
      const unit = await page.locator('[aria-label="Unit"]').first().boundingBox()
      check(
        unit.width >= 80 && unit.height >= (phone ? 40 : 32),
        'editor: the unit selector is readable and tappable',
        `${Math.round(unit.width)}x${Math.round(unit.height)}`,
      )
      check(
        (await page.locator('[data-amount-weight]').first().innerText()).includes('g'),
        'editor: every ingredient row still says its weight',
      )
      // The thing the old row lost entirely: which ingredient a line is.
      const names = await page.$$eval('ul.divide-y > li > div:first-child', (els) =>
        els.map((el) => el.getBoundingClientRect().width),
      )
      check(
        names.every((w) => w >= (phone ? 150 : 200)),
        'editor: every row says which ingredient it is',
        names.map(Math.round).join(', '),
      )
      if (phone) await tappable(page, 'editor', 'ul.divide-y [aria-label^="Remove "]')
    })

    // The add-ingredient dialog.
    await scene(
      'dialog',
      async (page) => {
        await page.goto(`${WEB}/recipes/${recipeId}`, { waitUntil: 'networkidle' })
        await touchReady(page)
        await page.getByRole('button', { name: 'Edit' }).first().click()
        await page.waitForSelector('#r-name')
        await page.getByRole('button', { name: 'Add ingredient' }).click()
        await page.waitForSelector('[data-slot="dialog-content"]')
        await page.waitForTimeout(350)
        await noSideScroll(page, 'add-ingredient dialog')
        const dialog = await page.locator('[data-slot="dialog-content"]').boundingBox()
        check(
          dialog.x >= -1 && dialog.x + dialog.width <= viewport.width + 1,
          'add-ingredient dialog: inside the screen',
          `${Math.round(dialog.x)}..${Math.round(dialog.x + dialog.width)}`,
        )
        // Three tabs of the picker inside three tabs of the dialog, over a
        // list of long food names: everything in there has to fit the box,
        // and the page behind it can be as wide as it likes.
        await within(page, 'add-ingredient dialog', '[data-slot="dialog-content"]', 'dialog')
      },
      false,
    )

    // The diary, for the recipe-shaped rows and the per-meal actions.
    await scene('diary', async (page) => {
      await page.goto(`${WEB}/diary`, { waitUntil: 'networkidle' })
      await touchReady(page)
      await page.waitForSelector('text=Breakfast')
      await noSideScroll(page, 'diary')
      await withinCards(page, 'diary')
      await noOverlap(page, 'diary', 'ul.divide-y > li')
      // Direct children only: the day card's "I logged everything" switch is
      // inside the label that is the thing you actually tap.
      if (phone) await tappable(page, 'diary', '[data-slot="card-action"] > button')

      // The point of the whole thing: the row says how much in the words it
      // was entered in, with the weight still beside it.
      const shown = await page.$$eval('[data-entry-amount]', (els) =>
        els.map((el) => el.textContent.trim()),
      )
      check(
        shown.some((t) => t.includes(said)),
        `diary: the entry row reads "${said}"`,
        shown.join(' | '),
      )
      // The amount is also the way back into it, so the whole left side of
      // the row is the target rather than a fifth icon on a full line.
      if (phone) await tappable(page, 'diary entry', 'li > [aria-label^="Edit "]', 36)
    })

    // The amount control, in the dialog it is mostly used in.
    //
    // Everything here is one screen wide: a count, a unit that has to spell
    // out a household measure, the weight it works out, and the macros under
    // that. A recent food comes pre-filled with the amount it was logged at
    // last time, which for this one is two of them.
    const openAmount = async (page, food = chicken) => {
      await page.goto(`${WEB}/diary`, { waitUntil: 'networkidle' })
      await touchReady(page)
      await page.waitForSelector('text=Dinner')
      // Dinner: breakfast, lunch, dinner, snack, in that order.
      await page.getByRole('button', { name: 'Add', exact: true }).nth(2).click()
      await page.waitForSelector('[data-slot="dialog-content"]')
      // Inside the dialog: the diary row behind it is a button with the same
      // name on it now, and the overlay was swallowing the click.
      await page
        .locator(`[data-slot="dialog-content"] button:has-text("${food}")`)
        .first()
        .click()
      await page.waitForSelector('#amount-count')
      await page.waitForTimeout(250)
    }

    await scene(
      'amount',
      async (page) => {
        await openAmount(page)
        await noSideScroll(page, 'amount control')
        await within(page, 'amount control', '[data-slot="dialog-content"]', 'dialog')
        const dialog = await page.locator('[data-slot="dialog-content"]').boundingBox()
        check(
          dialog.x >= -1 && dialog.x + dialog.width <= viewport.width + 1,
          'amount control: the dialog is inside the screen',
          `${Math.round(dialog.x)}..${Math.round(dialog.x + dialog.width)}`,
        )

        const min = phone ? 40 : 32
        const count = await page.locator('#amount-count').boundingBox()
        check(
          count.width >= 56 && count.height >= min,
          'amount control: the count box is tappable',
          `${Math.round(count.width)}x${Math.round(count.height)}`,
        )
        const unit = await page.locator('[aria-label="Unit"]').boundingBox()
        check(
          unit.width >= 90 && unit.height >= min,
          'amount control: the unit selector is tappable',
          `${Math.round(unit.width)}x${Math.round(unit.height)}`,
        )

        // The exact number, which is the thing the grams were kept for.
        const weight = page.locator('[data-amount-weight]')
        check(await weight.isVisible(), 'amount control: the weight is on screen')
        const shown = (await weight.innerText()).trim()
        check(
          shown.includes('348 g'),
          'amount control: two of them comes to 348 g',
          `${await page.locator('#amount-count').inputValue()} → ${shown}`,
        )

        // A box that already holds a 1 is how "2" becomes "12". Typing a
        // two-digit count has to give that count and nothing else.
        await page.locator('#amount-count').click()
        await page.locator('#amount-count').pressSequentially('12', { delay: 40 })
        const typed = await page.locator('#amount-count').inputValue()
        check(typed === '12', 'amount control: a two-digit count types as itself', typed)
        check(
          (await weight.innerText()).includes('2,088 g') ||
            (await weight.innerText()).includes('2088 g'),
          'amount control: the weight follows what was typed',
          await weight.innerText(),
        )
        await page.locator('#amount-count').fill('2')
      },
      false,
    )

    // Saying how much one of something is, without leaving the dialog. On the
    // food that has none: most foods have none, and a measure nobody can add
    // while logging is a settings page nobody fills in.
    await scene(
      'measure',
      async (page) => {
        await openAmount(page, burrito)
        await page.locator('[aria-label="Unit"]').click()
        await page.waitForSelector('[data-slot="select-item"]')
        await page.waitForTimeout(250)
        const items = await page.$$eval('[data-slot="select-item"]', (els) =>
          els.map((el) => el.getBoundingClientRect()),
        )
        check(
          items.every((r) => r.left >= -1 && r.right <= viewport.width + 1),
          'amount control: the unit list opens on screen',
          items.map((r) => `${Math.round(r.left)}..${Math.round(r.right)}`).join(' '),
        )
        await page.getByRole('option', { name: /Add a measure/ }).click()
        await page.waitForSelector('[data-amount-new]')
        await page.waitForTimeout(250)
        await noSideScroll(page, 'new measure')
        await within(page, 'new measure', '[data-slot="dialog-content"]', 'dialog')
        for (const [label, id] of [
          ['name', '#amount-new-label'],
          ['weight', '#amount-new-grams'],
        ]) {
          const box = await page.locator(id).boundingBox()
          check(
            box.width >= 60 && box.height >= (phone ? 40 : 32),
            `new measure: the ${label} box is tappable`,
            `${Math.round(box.width)}x${Math.round(box.height)}`,
          )
          check(
            (await page.locator(id).inputValue()) === '',
            `new measure: the ${label} box starts empty`,
          )
        }
        // And it saves, and is picked: adding a measure is something you do
        // in order to use it, not in order to have added it.
        //
        // Its own name per pass. One fixture is built for all six passes, and
        // a portion added in the first is still on the food in the second —
        // where adding it again is a duplicate the food form rightly refuses.
        const measure = `pack ${tag}`
        await page.locator('#amount-new-label').fill(measure)
        await page.locator('#amount-new-grams').fill('220')
        await page.getByRole('button', { name: 'Save and use it' }).click()
        await page.waitForSelector('[aria-label="Unit"]')
        await page.waitForTimeout(400)
        check(
          (await page.locator('[aria-label="Unit"]').innerText()).includes(measure),
          'new measure: the new measure is the one selected',
          await page.locator('[aria-label="Unit"]').innerText(),
        )
        check(
          (await page.locator('[data-amount-weight]').innerText()).includes('220 g'),
          'new measure: and one of them weighs what was typed',
          await page.locator('[data-amount-weight]').innerText(),
        )
      },
      false,
    )

    // The rest of the app, which shares the button and dialog primitives the
    // recipe screens were fixed with: nothing more than a check that widening
    // every touch control did not push a page sideways.
    const rest = await context.newPage()
    for (const path of ['/', '/foods', '/weight', '/settings/display']) {
      await rest.goto(`${WEB}${path}`, { waitUntil: 'networkidle' })
      await noSideScroll(rest, `page ${path}`)
    }
    await rest.close()

    await context.close()
  }
}

await browser.close()

console.log(`\n${failures === 0 ? 'all good' : `${failures} failure(s)`} — screenshots in ${SHOTS}/`)
process.exit(failures === 0 ? 0 : 1)
