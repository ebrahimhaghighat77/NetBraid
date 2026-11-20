import { describe, expect, it } from "vitest";
import { linkColors } from "./colors";
import { sampleLink, sampleSnapshot } from "./fixtures";
import { bytes, count, rate, splitBytes } from "./format";
import { capPercent, capText, linkState } from "./netstate";
import { SAMPLES, totals, window60 } from "./stream";
import { footerText, headline } from "./summary";

describe("format", () => {
  it("uses decimal units with about three digits", () => {
    expect(bytes(0)).toBe("0 B");
    expect(bytes(1_500)).toBe("1.5 KB");
    expect(bytes(3_400_000_000)).toBe("3.4 GB");
    expect(splitBytes(999_960)).toEqual(["1.0", "MB"]);
    expect(rate(250_000)).toBe("250 KB/s");
    expect(count(1, "network")).toBe("1 network");
    expect(count(1200, "connection")).toBe("1,200 connections");
  });
});

describe("colors", () => {
  it("gives a second network of a kind a spare colour", () => {
    const colors = linkColors([
      sampleLink({ id: "b", kind: "ethernet" }),
      sampleLink({ id: "a", kind: "ethernet" }),
    ]);
    expect(colors.get("a")).toBe("var(--net-eth)");
    expect(colors.get("b")).toBe("var(--net-spare-1)");
  });
});

describe("network state", () => {
  it("explains why a network is not used", () => {
    expect(linkState(sampleLink({ present: false, kind: "ethernet" }), true).detail).toBe("Cable unplugged");
    expect(linkState(sampleLink({ capped: true }), true).detail).toBe("Data cap reached");
    expect(linkState(sampleLink({ reach: "down" }), true).detail).toBe("No internet");
    expect(linkState(sampleLink({ backup: true }), true).detail).toBe("Standing by");
    expect(linkState(sampleLink({ rxRate: 2_000_000, share: 60, connections: 4 }), true)).toEqual({
      value: "2.0 MB/s",
      detail: "60% of traffic",
    });
    expect(linkState(sampleLink(), false).value).toBe("40 ms");
  });

  it("measures data caps in decimal megabytes", () => {
    const link = sampleLink({ capMb: 20_000, usedBytes: 5_000_000_000 });
    expect(capText(link)).toBe("5.0 GB of 20.0 GB this month");
    expect(capPercent(link)).toBe(25);
  });
});

describe("stream", () => {
  it("pads history to a minute and stacks the networks", () => {
    const a = window60([1, 2]);
    expect(a).toHaveLength(SAMPLES);
    expect(a.slice(-2)).toEqual([1, 2]);
    expect(totals([a, window60([5])]).slice(-2)).toEqual([1, 7]);
  });
});

describe("summary", () => {
  const wifi = sampleLink({ isDefault: true });
  const phone = sampleLink({ id: "{phone}", name: "Phone", kind: "phone" });

  it("says what NetBraid is doing", () => {
    expect(headline(sampleSnapshot({ links: [wifi, phone] }), null).text).toBe("Off, Windows uses Wi-Fi alone");
    expect(headline(sampleSnapshot({ on: true, links: [wifi, phone] }), null)).toEqual({
      text: "Combining 2 networks",
      tone: "ok",
    });
    expect(headline(sampleSnapshot({ on: true, links: [wifi] }), null).text).toBe("On, using 1 network");
    expect(headline(sampleSnapshot(), "on").text).toBe("Turning on…");
    expect(headline(sampleSnapshot({ on: true, links: [sampleLink({ reach: "down" })] }), null).tone).toBe("warn");
  });

  it("sums up the footer", () => {
    expect(footerText(sampleSnapshot({ on: true, connections: 42, todayBytes: 3_400_000_000 }))).toBe(
      "42 connections, 3.4 GB today",
    );
    expect(footerText(sampleSnapshot({ links: [wifi, phone] }))).toBe("2 networks ready");
  });
});
