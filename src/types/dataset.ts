export type ResizeFilter = "nearest" | "triangle" | "catmullrom" | "gaussian" | "lanczos3";

export interface DatasetRule {
  id: string;
  recipe_id: string;
  position: number;
  label: string | null;
  format: "jpeg" | "png";
  quality: number;
  /** 输出像素预算上限（百万像素，如 0.5）；null = 不限制。 */
  max_megapixels: number | null;
  resize_filter: ResizeFilter;
}

export interface DatasetRecipe {
  id: string;
  collection_id: string;
  caption_mode: "all" | "manual" | "ai" | "latest";
  naming_template: string;
  rules: DatasetRule[];
}

/** 提交形态：规则由后端重新生成 id。 */
export interface DatasetRuleInput {
  label: string | null;
  format: "jpeg" | "png";
  quality: number;
  max_megapixels: number | null;
  resize_filter: ResizeFilter | null;
}

export interface DatasetRecipeInput {
  caption_mode: "all" | "manual" | "ai" | "latest";
  naming_template: string;
  rules: DatasetRuleInput[];
}

/**
 * 单条目覆盖。caption_mode 为 null 表示跟随配方；
 * enabled_rules 为 null 表示全部规则，[] 表示全部禁用。
 */
export interface DatasetItemOverride {
  collection_id: string;
  media_id: string;
  caption_mode: "all" | "manual" | "ai" | "latest" | null;
  enabled_rules: string[] | null;
}
