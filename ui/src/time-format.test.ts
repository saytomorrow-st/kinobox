import { describe, expect, it } from "vitest";

import { formatTime } from "./time-format";

describe("formatTime", () => {
  it("неизвестное время (null) показывает прочерки", () => {
    expect(formatTime(null)).toBe("--:--");
  });

  it("ноль и короткие значения в формате MM:SS", () => {
    expect(formatTime(0)).toBe("00:00");
    expect(formatTime(5)).toBe("00:05");
    expect(formatTime(65)).toBe("01:05");
  });

  it("59:59 ещё без часов", () => {
    expect(formatTime(3599)).toBe("59:59");
  });

  it("от 3600 секунд переходит на формат H:MM:SS", () => {
    expect(formatTime(3600)).toBe("1:00:00");
    expect(formatTime(3661)).toBe("1:01:01");
  });

  it("отрицательное время зажимается в ноль", () => {
    expect(formatTime(-5)).toBe("00:00");
  });

  it("дробные секунды округляются вниз", () => {
    expect(formatTime(90.7)).toBe("01:30");
  });
});
