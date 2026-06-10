import { describe, it, expect, beforeEach } from "vitest";
import { isMobile, isDesktop, MOBILE_BREAKPOINT } from "./deviceDetection";

describe("MOBILE_BREAKPOINT", () => {
  it("is 768", () => {
    expect(MOBILE_BREAKPOINT).toBe(768);
  });
});

describe("isMobile", () => {
  beforeEach(() => {
    Object.defineProperty(window, "innerWidth", {
      writable: true,
      configurable: true,
      value: 1024,
    });
  });

  it("returns true when width equals breakpoint", () => {
    Object.defineProperty(window, "innerWidth", { value: 768 });
    expect(isMobile()).toBe(true);
  });

  it("returns true when width is below breakpoint", () => {
    Object.defineProperty(window, "innerWidth", { value: 375 });
    expect(isMobile()).toBe(true);
  });

  it("returns false when width is above breakpoint", () => {
    Object.defineProperty(window, "innerWidth", { value: 1024 });
    expect(isMobile()).toBe(false);
  });
});

describe("isDesktop", () => {
  it("returns true when width is above breakpoint", () => {
    Object.defineProperty(window, "innerWidth", {
      writable: true,
      configurable: true,
      value: 1280,
    });
    expect(isDesktop()).toBe(true);
  });

  it("returns false when width equals breakpoint", () => {
    Object.defineProperty(window, "innerWidth", { value: 768 });
    expect(isDesktop()).toBe(false);
  });

  it("returns false when width is below breakpoint", () => {
    Object.defineProperty(window, "innerWidth", { value: 320 });
    expect(isDesktop()).toBe(false);
  });

  it("is the inverse of isMobile", () => {
    const widths = [320, 768, 1024, 1440];
    for (const width of widths) {
      Object.defineProperty(window, "innerWidth", { value: width });
      expect(isDesktop()).toBe(!isMobile());
    }
  });
});
