import { describe, expect, it } from "vitest";
import {
  asLink,
  calculate,
  convert,
  formatNumber,
  matchApps,
  matchWindows,
  webSearchUrl,
} from "./launcher";

describe("calculator", () => {
  it("works out sums in the usual order", () => {
    expect(calculate("2 + 3 * 4")).toBe(14);
    expect(calculate("(2 + 3) * 4")).toBe(20);
    expect(calculate("2 ^ 3 ^ 2")).toBe(512);
    expect(calculate("-5 + 2")).toBe(-3);
    expect(calculate("10 / 4")).toBe(2.5);
    expect(calculate("1,5 * 2")).toBe(3);
    expect(calculate("7 × 6")).toBe(42);
    expect(calculate("200 * 15%")).toBe(30);
    expect(calculate("15% of 80")).toBe(12);
  });

  it("leaves words and bare numbers alone", () => {
    expect(calculate("firefox")).toBeNull();
    expect(calculate("42")).toBeNull();
    expect(calculate("-42")).toBeNull();
    expect(calculate("2 +")).toBeNull();
    expect(calculate("(2 + 3")).toBeNull();
    expect(calculate("1 / 0")).toBeNull();
    expect(calculate("alert(1)")).toBeNull();
  });

  it("shows results tidily", () => {
    expect(formatNumber(0.1 + 0.2)).toBe("0.3");
    expect(formatNumber(10 / 3)).toBe("3.333333");
    expect(formatNumber(1e20)).toBe("1.0000e+20");
  });
});

describe("unit conversion", () => {
  it("converts within a kind", () => {
    expect(convert("5 km in miles")).toBe("5 km = 3.106856 miles");
    expect(convert("3 kg to lb")).toBe("3 kg = 6.613868 lb");
    expect(convert("90 min to hours")).toBe("90 min = 1.5 hours");
    expect(convert("2 GB in MB")).toBe("2 GB = 2048 MB");
    expect(convert("100 km/h to mph")).toBe("100 km/h = 62.137119 mph");
    expect(convert("20 c to f")).toBe("20 °C = 68 °F");
    expect(convert("-40 f to c")).toBe("-40 °F = -40 °C");
  });

  it("says nothing when it makes no sense", () => {
    expect(convert("5 km in kg")).toBeNull();
    expect(convert("5 bananas in apples")).toBeNull();
    expect(convert("km in miles")).toBeNull();
  });
});

describe("apps and the web", () => {
  const apps = [
    "Firefox",
    "Files",
    "LibreOffice Writer",
    "Visual Studio Code",
    "Fire Alarm Test",
  ].map((name) => ({
    name,
    path: `/apps/${name}`,
  }));

  it("puts names that start with the text first", () => {
    // "fi" also hides inside "LibreOffice": names that only contain it come last.
    expect(matchApps(apps, "fi").map((a) => a.name)).toEqual([
      "Files",
      "Firefox",
      "Fire Alarm Test",
      "LibreOffice Writer",
    ]);
    expect(matchApps(apps, "code").map((a) => a.name)).toEqual([
      "Visual Studio Code",
    ]);
    expect(matchApps(apps, "office").map((a) => a.name)).toEqual([
      "LibreOffice Writer",
    ]);
    expect(matchApps(apps, "  ")).toEqual([]);
    expect(matchApps(apps, "zzz")).toEqual([]);
  });

  it("searches the web, or opens an address", () => {
    expect(webSearchUrl(" rust & tauri ")).toBe(
      "https://duckduckgo.com/?q=rust%20%26%20tauri",
    );
    expect(asLink("example.com/page")).toBe("https://example.com/page");
    expect(asLink("https://example.com")).toBe("https://example.com");
    expect(asLink("http://example.com")).toBeNull();
    expect(asLink("two words.com")).toBeNull();
    expect(asLink("firefox")).toBeNull();
  });
});

describe("matchWindows", () => {
  const windows = [
    { id: "1", title: "Inbox - Thunderbird", app: "thunderbird" },
    { id: "2", title: "Budget.ods - LibreOffice Calc", app: "libreoffice" },
    { id: "3", title: "Thesis draft", app: "Writer" },
  ];
  it("finds a window by its app or title, starts first", () => {
    expect(matchWindows(windows, "thun").map((w) => w.id)).toEqual(["1"]);
    expect(matchWindows(windows, "the").map((w) => w.id)).toEqual(["3"]);
    expect(matchWindows(windows, "calc").map((w) => w.id)).toEqual(["2"]);
    expect(matchWindows(windows, " ")).toEqual([]);
  });
});
