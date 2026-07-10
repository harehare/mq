import { describe, it, expect, afterEach } from "vitest";
import {
  MOBILE_BREAKPOINT,
  isMobile,
  isDesktop,
} from "../../src/utils/deviceDetection";

const setInnerWidth = (width: number) => {
  Object.defineProperty(window, "innerWidth", {
    writable: true,
    configurable: true,
    value: width,
  });
};

describe("deviceDetection", () => {
  afterEach(() => {
    setInnerWidth(1024);
  });

  it("treats width below the breakpoint as mobile", () => {
    setInnerWidth(MOBILE_BREAKPOINT - 1);
    expect(isMobile()).toBe(true);
    expect(isDesktop()).toBe(false);
  });

  it("treats width equal to the breakpoint as mobile", () => {
    setInnerWidth(MOBILE_BREAKPOINT);
    expect(isMobile()).toBe(true);
    expect(isDesktop()).toBe(false);
  });

  it("treats width above the breakpoint as desktop", () => {
    setInnerWidth(MOBILE_BREAKPOINT + 1);
    expect(isMobile()).toBe(false);
    expect(isDesktop()).toBe(true);
  });
});
