export type VariantVisibility = "representative" | "all";
export type BrowseItemKind = "original" | "variant";

export interface BrowseItem {
  item_id: string;
  item_kind: BrowseItemKind;
  media_id: string;
  variant_id: string | null;
  is_display_variant: boolean;
  source_path: string | null;
  width: number | null;
  height: number | null;
  file_size: number | null;
  created_at: string | null;
  modified_at: string | null;
  imported_at: string;
  source_url: string | null;
  page_url: string | null;
  source: string | null;
  sha256: string | null;
  deleted_at: string | null;
  display_variant_id: string | null;
  thumb_256: string | null;
  lqip: string | null;
  media_type: string | null;
  duration: number | null;
  video_codec: string | null;
  video_fps: number | null;
  label: string | null;
  preset_name: string | null;
}
