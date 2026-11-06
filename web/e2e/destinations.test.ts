// The bar's slots and the More sheet (`web/src/shell/nav.ts`) reaching the pages `main.ts` routes them to.

import assert from "node:assert/strict";

import { BASE, DESKTOP, PHONE, shownTodos, suite } from "./harness";

const app = await suite("destinations");

const barLabels = async (): Promise<string[]> =>
  app.page.evaluate<string[]>(
    `[...document.querySelectorAll(".tabbar > .tab")].map((tab) => tab.querySelector(".tablabel")?.textContent ?? "")`,
  );

app.scene("every slot in the bar opens a page the router knows", async () => {
  await app.open(BASE);
  for (const [label, hash] of [
    ["Todos", "#/"],
    ["Inbox", "#/inbox"],
    ["Secretary", "#/secretary/default"],
    ["Briefing", "#/briefings"],
  ] as const) {
    await app.page.navigate(`${BASE}/${hash}`);
    await app.page.waitFor(`document.querySelector("#view .pagetitle, #view h1") !== null`);
    const title = await app.page.evaluate<string>(
      `document.querySelector("#view .pagetitle, #view h1").textContent`,
    );
    assert.notEqual(title, "No such page", `${label} at ${hash} is not routed`);
    const lit = await app.page.evaluate<string>(
      `document.querySelector('.tabbar .tab.active .tablabel')?.textContent ?? ""`,
    );
    assert.equal(lit, label, `${hash} should light ${label}`);
  }
});

app.scene("Project is the OTHER chain, not the same list under a new name", async () => {
  await app.open(BASE);
  const life = await shownTodos(app.page);
  assert.ok(life.length > 0, "the first chain has todos");

  await app.page.navigate(`${BASE}/#/project`);
  await app.page.waitFor(`document.querySelector("#view .pagetitle")?.textContent === "Project"`);
  // The note, not a count of zero cards: a list that has not drawn yet also has none.
  await app.page.waitFor(`document.querySelector(".todospane .empty:not([hidden])") !== null`);
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".tree .row").length`), 0);

  // The fixture's second chain holds one resolved todo, so nothing in it is open.
  const said = await app.page.evaluate<string>(`document.querySelector(".todospane .empty:not([hidden])").textContent`);
  assert.match(said, /Nothing is open/, `the empty note should name the reason this list is empty: ${said}`);
  assert.equal(said.includes("filter"), false, "nobody narrowed a filter; do not tell them to widen one");
});

app.scene("a page whose code arrives after the next tap is never drawn over it", async () => {
  // A fresh document, so the Rust pages' module is fetched again, and held.
  const release = await app.page.holdRequests(["views_bg.wasm"]);
  const fresh = `${BASE}/?fresh=${Date.now()}`;
  await app.page.navigate(`${fresh}#/briefings`);
  await app.page.untilHeld();
  await app.page.navigate(`${fresh}#/project`);
  // The list is a Rust page too, so both wait on the module: every title drawn from the tap on is recorded.
  await app.page.evaluate(`(() => {
    const titles = (window.__titles = []);
    const view = document.getElementById("view");
    // From each record's added nodes, since one callback can hear two draws.
    new MutationObserver((records) => {
      for (const r of records)
        for (const n of r.addedNodes)
          if (n instanceof Element) for (const t of [n, ...n.querySelectorAll(".pagetitle")]) if (t.matches(".pagetitle")) titles.push(t.textContent ?? "");
    }).observe(view, { subtree: true, childList: true });
  })()`);
  await release();
  await app.page.waitFor(`performance.getEntriesByName("views:load").length > 0`);
  await app.page.waitFor(`document.querySelector("#view .pagetitle")?.textContent === "Project"`);
  // The page waiting on the module draws, or is dropped, in the microtasks after it loads: a task later, either is done.
  await app.page.evaluate(`new Promise((done) => setTimeout(done, 0))`);
  assert.equal(await app.page.evaluate<string>(`document.querySelector("#view .pagetitle")?.textContent ?? ""`), "Project");
  const drawn = await app.page.evaluate<string[]>(`window.__titles`);
  assert.ok(!drawn.includes("Briefings"), `the page tapped away from was never drawn: ${JSON.stringify(drawn)}`);
});

/** Open the More sheet on whatever hash the reader is standing on. */
const openMore = async (): Promise<void> => {
  await app.page.evaluate(
    `[...document.querySelectorAll(".tabbar .tab")].find((tab) => tab.textContent?.includes("More"))?.click()`,
  );
  await app.page.waitFor(`document.querySelector(".moresheet")?.hidden === false`);
};

app.scene("at 62rem the same words sit across the top, and More holds the rest", async () => {
  await app.resize(DESKTOP);
  await app.open(BASE);
  assert.deepEqual(await barLabels(), ["Todos", "Inbox", "Secretary", "Briefing", "More"]);
  const bar = await app.page.evaluate<{ top: number; position: string }>(
    `(() => { const bar = document.querySelector("header .tabbar"); return { top: bar.getBoundingClientRect().top, position: getComputedStyle(bar).position }; })()`,
  );
  assert.ok(bar.top < 80, `the bar is at the top of a wide screen, not its foot (top ${bar.top})`);
  assert.notEqual(bar.position, "fixed");
  await openMore();
  const rest = await app.page.evaluate<string[]>(
    `[...document.querySelectorAll(".moresheet .morelink .tablabel")].map((n) => n.textContent)`,
  );
  assert.deepEqual(rest, ["Project", "Mail", "Calendar", "Settings", "About"]);
  await app.page.evaluate(`document.body.click()`);
  await app.page.waitFor(`document.querySelector(".moresheet")?.hidden === true`);
  await app.resize(PHONE);
});

// The URL `MainActivity.targetOf` hands the WebView for a share. Its hash goes back to the list's, so sharing the same
// link again is a hash change.
app.scene("a shared link opens the list with the sheet prefilled, once", async () => {
  // The share hashes are hash changes, not navigations: consuming one rewrites the URL `navigate` waits for.
  await app.open(BASE);
  await app.page.navigate(`${BASE}/#/add`);
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector("#view .pagetitle").textContent`),
    "Todos",
    "#/add is routed — an unrouted hash lands on No such page quietly",
  );
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".addsheet.open") !== null`), false);

  const shared = "#/add?text=https%3A%2F%2Fexample.test%2Ffpl%3Fv%3D2&title=The%20FPL%20paper";
  const share = async (): Promise<void> => {
    await app.page.evaluate(`window.location.hash = ${JSON.stringify(shared)}`);
    await app.page.waitFor(`document.querySelector(".addsheet.open") !== null`, { timeout: 8000 });
  };

  await share();
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".draftbody").value`),
    "The FPL paper\nhttps://example.test/fpl?v=2",
    "the two shared fields, composed by draft.sharedBody and nothing else",
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".tabbar .tab.active .tablabel")?.textContent ?? ""`),
    "Todos",
    "a share lights the list it lands on, not nothing",
  );
  assert.equal(
    await app.page.evaluate<string>(`window.location.hash`),
    "#/",
    "the share is consumed: the hash goes back to the list's",
  );

  await app.page.evaluate(`document.querySelector(".scrim").click()`);
  await app.page.waitFor(`document.querySelector(".addsheet.open") === null`);
  await share();
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".draftbody").value`),
    "The FPL paper\nhttps://example.test/fpl?v=2",
  );

  await app.page.evaluate(`document.querySelector(".scrim").click()`);
});

const pickTheme = async (label: string): Promise<void> => {
  await openMore();
  await app.page.clickText(".moresheet .themepick", label);
  await app.page.waitFor(`document.querySelector(".moresheet .themepick.active")?.textContent === ${JSON.stringify(label)}`);
  await app.page.evaluate(`document.body.click()`);
  await app.page.waitFor(`document.querySelector(".moresheet")?.hidden === true`);
};

app.scene("the theme override wins over the machine, in both directions", async () => {
  await app.page.colorScheme("dark");
  await app.open(BASE);
  const ground = async (): Promise<string> =>
    app.page.evaluate<string>(`getComputedStyle(document.body).backgroundColor`);
  const onDarkMachine = await ground();

  await pickTheme("Light");
  await app.page.waitFor(`document.documentElement.getAttribute("data-theme") === "light"`);
  const forcedLight = await ground();
  assert.notEqual(forcedLight, onDarkMachine, "an override that changes nothing is not an override");

  await app.page.reload();
  await app.page.waitFor(`document.documentElement.getAttribute("data-theme") === "light"`);
  assert.equal(await ground(), forcedLight);

  await pickTheme("System");
  await app.page.waitFor(`document.documentElement.getAttribute("data-theme") === null`);
  assert.equal(await ground(), onDarkMachine);

  await app.page.colorScheme(null);
});
