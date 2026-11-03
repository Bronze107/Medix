import { describe, it, expect } from "vitest";
import { pickThumbnailTier, BASE_TIER } from "./thumbnailTier";

describe("pickThumbnailTier", () => {
  it("需求不超过基础档时直接返回基础档", () => {
    expect(pickThumbnailTier(1, 4000)).toBe(BASE_TIER);
    expect(pickThumbnailTier(200, 4000)).toBe(BASE_TIER);
    expect(pickThumbnailTier(256, 4000)).toBe(BASE_TIER);
  });

  it("在可用高档位中选最小的满足档", () => {
    expect(pickThumbnailTier(257, 4000)).toBe(512);
    expect(pickThumbnailTier(512, 4000)).toBe(512);
    expect(pickThumbnailTier(513, 4000)).toBe(1024);
    expect(pickThumbnailTier(1024, 4000)).toBe(1024);
  });

  it("需求超过最大档位时钳制到最大档", () => {
    expect(pickThumbnailTier(2000, 4000)).toBe(1024);
    expect(pickThumbnailTier(9999, 4000)).toBe(1024);
  });

  it("高档位不得超过原图最长边", () => {
    // 原图 600px：512 可用，1024 超原图被排除
    expect(pickThumbnailTier(800, 600)).toBe(512);
    expect(pickThumbnailTier(1024, 600)).toBe(512);
    // 原图 300px：没有任何可用高档位，回到基础档
    expect(pickThumbnailTier(800, 300)).toBe(BASE_TIER);
    expect(pickThumbnailTier(2000, 256)).toBe(BASE_TIER);
  });

  it("原图尺寸未知/非法时按只有基础档处理", () => {
    expect(pickThumbnailTier(800, null)).toBe(BASE_TIER);
    expect(pickThumbnailTier(800, undefined)).toBe(BASE_TIER);
    expect(pickThumbnailTier(800, 0)).toBe(BASE_TIER);
    expect(pickThumbnailTier(800, -5)).toBe(BASE_TIER);
  });

  it("非有限/小数需求像素被归一化", () => {
    expect(pickThumbnailTier(NaN, 4000)).toBe(BASE_TIER);
    expect(pickThumbnailTier(Infinity, 4000)).toBe(BASE_TIER);
    expect(pickThumbnailTier(256.4, 4000)).toBe(512);
    expect(pickThumbnailTier(-10, 4000)).toBe(BASE_TIER);
  });
});
