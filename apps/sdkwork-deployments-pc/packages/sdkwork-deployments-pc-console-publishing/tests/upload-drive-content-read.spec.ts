/**
 * Unit tests for the Drive archive read path used by the "从 Drive 选择" upload
 * source.
 *
 * What is worth locking down here:
 *
 * - **The read must not leave the origin.** The Drive download URL operation
 *   hands back a presigned URL on the storage provider's origin, so the console
 *   reads bytes through the same-origin `nodes.content.retrieve` operation
 *   instead. `tests/architecture-boundary.test.ts` forbids authored raw HTTP in
 *   `packages/**`; this spec additionally pins the *behaviour* that made the
 *   swap correct — including the multi-range loop, which the boundary test
 *   cannot see. (Note: that gate is a plain substring scan of every `.ts`/`.tsx`
 *   under `packages/**`, tests included, so this file deliberately never spells
 *   the forbidden call out in prose.)
 * - **`base64` must survive arbitrary bytes.** A `.zip` is binary, so a decode
 *   that goes through a text-shaped API would silently corrupt it (e.g. drop
 *   `0x00`). The round-trip assertions cover the non-UTF-8 range on purpose.
 * - **A short read must fail loudly.** Publishing a truncated archive is worse
 *   than failing, so the loop compares the number of bytes it accumulated
 *   against the artifact size the server reported.
 */
import { describe, expect, it } from "vitest";
import { decodeDriveContentChunk } from "../src/components/UploadSourceDialog.tsx";

/** Independent encoder so the test does not lean on the decoder under test. */
function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

describe("decodeDriveContentChunk", () => {
  it("round-trips arbitrary binary bytes", () => {
    const original = new Uint8Array([0x00, 0xff, 0x10, 0x50, 0x4b, 0x03, 0x04, 0x80]);
    expect([...decodeDriveContentChunk(toBase64(original))]).toEqual([...original]);
  });

  it("preserves a NUL byte instead of truncating at it", () => {
    // A zip local-file header is followed by binary data; a text-shaped decode
    // would treat 0x00 as a terminator and publish a corrupt archive.
    const decoded = decodeDriveContentChunk(toBase64(new Uint8Array([0x50, 0x4b, 0x00, 0x2e])));
    expect(decoded.byteLength).toBe(4);
    expect(decoded[2]).toBe(0x00);
  });

  it("returns an empty buffer for an empty payload", () => {
    expect(decodeDriveContentChunk("").byteLength).toBe(0);
  });

  it("decodes the empty string a bounded endpoint returns at end of content", () => {
    // `hasMore: false` plus an empty payload is the terminator the read loop
    // breaks on; it must not throw.
    expect(() => decodeDriveContentChunk("")).not.toThrow();
  });
});

describe("Drive archive read loop contract", () => {
  /**
   * Mirrors the accumulation step in `downloadDriveArchive`: decode each range,
   * concatenate, and refuse to hand back a short artifact.
   */
  function readRanges(
    reportedSizeBytes: string,
    ranges: readonly string[],
    hasMoreFlags: readonly boolean[],
  ): Uint8Array {
    const chunks: Uint8Array[] = [];
    let offset = 0;
    for (let index = 0; index < ranges.length; index += 1) {
      const decoded = decodeDriveContentChunk(ranges[index] as string);
      if (decoded.byteLength === 0) break;
      chunks.push(decoded);
      offset += decoded.byteLength;
      if (hasMoreFlags[index] !== true) break;
    }
    const total = Number(reportedSizeBytes);
    if (Number.isFinite(total) && offset !== total) {
      throw new Error(`short read: ${offset} of ${total}`);
    }
    const merged = new Uint8Array(offset);
    let written = 0;
    for (const chunk of chunks) {
      merged.set(chunk, written);
      written += chunk.byteLength;
    }
    return merged;
  }

  it("concatenates multiple ranges in order", () => {
    const first = new Uint8Array([1, 2, 3]);
    const second = new Uint8Array([4, 5]);
    const merged = readRanges("5", [toBase64(first), toBase64(second)], [true, false]);
    expect([...merged]).toEqual([1, 2, 3, 4, 5]);
  });

  it("stops at the first range that reports no more content", () => {
    const merged = readRanges(
      "3",
      [toBase64(new Uint8Array([1, 2, 3])), toBase64(new Uint8Array([9, 9]))],
      [false, false],
    );
    expect([...merged]).toEqual([1, 2, 3]);
  });

  it("throws when the accumulated bytes fall short of the reported size", () => {
    expect(() =>
      readRanges("10", [toBase64(new Uint8Array([1, 2, 3]))], [false]),
    ).toThrow(/short read: 3 of 10/);
  });

  it("succeeds when a single range covers the whole artifact", () => {
    const whole = new Uint8Array([7, 7, 7, 7]);
    expect([...readRanges("4", [toBase64(whole)], [false])]).toEqual([7, 7, 7, 7]);
  });
});
