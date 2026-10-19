import { describe, it, expect } from "vitest";
import { pickAiEditMediaIds } from "./mediaSelection";
import type { BrowseItem } from "@/types/browse";

function item(id: string, mediaId: string, mediaType: string | null): BrowseItem {
  return {
    id,
    media_id: mediaId,
    source_path: null,
    width: null,
    height: null,
    file_size: null,
    created_at: null,
    modified_at: null,
    imported_at: "",
    source_url: null,
    page_url: null,
    source: null,
    sha256: null,
    deleted_at: null,
    thumb_256: null,
    lqip: null,
    media_type: mediaType,
    duration: null,
    video_codec: null,
    video_fps: null,
    child_count: 0,
    parent_count: 0,
  };
}

describe("pickAiEditMediaIds", () => {
  it("maps browse-item ids to media ids in browse display order", () => {
    const items = [
      item("b3", "m1", "image"),
      item("b1", "m2", "image"),
      item("b2", "m3", "image"),
    ];
    // 顺序跟随 items（显示顺序），与选中集合的插入顺序无关
    expect(pickAiEditMediaIds(["b1", "b2", "b3"], items)).toEqual(["m1", "m2", "m3"]);
  });

  it("drops videos and keeps images", () => {
    const items = [
      item("b1", "m1", "image"),
      item("b2", "m2", "video"),
      item("b3", "m3", "image"),
    ];
    expect(pickAiEditMediaIds(["b1", "b2", "b3"], items)).toEqual(["m1", "m3"]);
  });

  it("returns empty when selection is all videos", () => {
    const items = [
      item("b1", "m1", "video"),
      item("b2", "m2", "video"),
    ];
    expect(pickAiEditMediaIds(["b1", "b2"], items)).toEqual([]);
  });

  it("dedupes variants sharing media_id, keeping first occurrence", () => {
    const items = [
      item("b1", "m1", "image"),
      item("b2", "m1", "image"),
      item("b3", "m2", "image"),
      item("b4", "m1", "image"),
    ];
    expect(pickAiEditMediaIds(["b4", "b3", "b2", "b1"], items)).toEqual(["m1", "m2"]);
  });

  it("skips stale ids missing from items", () => {
    const items = [item("b1", "m1", "image")];
    expect(pickAiEditMediaIds(["gone", "b1"], items)).toEqual(["m1"]);
  });

  it("returns empty for empty selection", () => {
    const items = [item("b1", "m1", "image")];
    expect(pickAiEditMediaIds([], items)).toEqual([]);
  });

  it("accepts any iterable (e.g. Set); order follows items, not selection", () => {
    const items = [
      item("b1", "m1", "image"),
      item("b2", "m2", "image"),
    ];
    expect(pickAiEditMediaIds(new Set(["b2", "b1"]), items)).toEqual(["m1", "m2"]);
  });
});
