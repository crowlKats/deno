// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.
import {
  assert,
  assertEquals,
  assertThrows,
  assertThrowsAsync,
  pathToAbsoluteFileUrl,
  unitTest,
} from "./test_util.ts";

unitTest({ perms: { read: true } }, function readFileSyncSuccess(): void {
  const data = Deno.readFileSync("cli/tests/fixture.json");
  assert(data.byteLength > 0);
  const decoder = new TextDecoder("utf-8");
  const json = decoder.decode(data);
  const pkg = JSON.parse(json);
  assertEquals(pkg.name, "deno");
});

unitTest({ perms: { read: true } }, function readFileSyncUrl(): void {
  const data = Deno.readFileSync(
    pathToAbsoluteFileUrl("cli/tests/fixture.json"),
  );
  assert(data.byteLength > 0);
  const decoder = new TextDecoder("utf-8");
  const json = decoder.decode(data);
  const pkg = JSON.parse(json);
  assertEquals(pkg.name, "deno");
});

unitTest({ perms: { read: false } }, function readFileSyncPerm(): void {
  assertThrows(() => {
    Deno.readFileSync("cli/tests/fixture.json");
  }, Deno.errors.PermissionDenied);
});

unitTest({ perms: { read: true } }, function readFileSyncNotFound(): void {
  assertThrows(() => {
    Deno.readFileSync("bad_filename");
  }, Deno.errors.NotFound);
});

unitTest({ perms: { read: true } }, async function readFileUrl(): Promise<
  void
> {
  const data = await Deno.readFile(
    pathToAbsoluteFileUrl("cli/tests/fixture.json"),
  );
  assert(data.byteLength > 0);
  const decoder = new TextDecoder("utf-8");
  const json = decoder.decode(data);
  const pkg = JSON.parse(json);
  assertEquals(pkg.name, "deno");
});

unitTest({ perms: { read: true } }, async function readFileSuccess(): Promise<
  void
> {
  const data = await Deno.readFile("cli/tests/fixture.json");
  assert(data.byteLength > 0);
  const decoder = new TextDecoder("utf-8");
  const json = decoder.decode(data);
  const pkg = JSON.parse(json);
  assertEquals(pkg.name, "deno");
});

unitTest({ perms: { read: false } }, async function readFilePerm(): Promise<
  void
> {
  await assertThrowsAsync(async () => {
    await Deno.readFile("cli/tests/fixture.json");
  }, Deno.errors.PermissionDenied);
});

unitTest({ perms: { read: true } }, function readFileSyncLoop(): void {
  for (let i = 0; i < 256; i++) {
    Deno.readFileSync("cli/tests/fixture.json");
  }
});

unitTest(
  { perms: { read: true } },
  async function readFileDoesNotLeakResources(): Promise<void> {
    const resourcesBefore = Deno.resources();
    await assertThrowsAsync(async () => await Deno.readFile("cli"));
    assertEquals(resourcesBefore, Deno.resources());
  },
);

unitTest(
  { perms: { read: true } },
  function readFileSyncDoesNotLeakResources(): void {
    const resourcesBefore = Deno.resources();
    assertThrows(() => Deno.readFileSync("cli"));
    assertEquals(resourcesBefore, Deno.resources());
  },
);

unitTest(
  { perms: { read: true } },
  async function readFileWithAbortSignal(): Promise<void> {
    const ac = new AbortController();
    queueMicrotask(() => ac.abort());
    await assertThrowsAsync(async () => {
      await Deno.readFile("cli/tests/fixture.json", { signal: ac.signal });
    });
  },
);

unitTest(
  { perms: { read: true } },
  async function readTextileWithAbortSignal(): Promise<void> {
    const ac = new AbortController();
    queueMicrotask(() => ac.abort());
    await assertThrowsAsync(async () => {
      await Deno.readTextFile("cli/tests/fixture.json", { signal: ac.signal });
    });
  },
);
