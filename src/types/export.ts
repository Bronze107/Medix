export interface ExportOptions {
  media_ids: string[];
  /** "recipe" 表示跟随数据集配方（仅集合导出时有效）。 */
  caption_mode: "all" | "manual" | "ai" | "latest" | "recipe";
  export_original: boolean;
  export_json: boolean;
  output_dir: string;
  use_zip: boolean;
  /** 数据集导出：展开集合成员并应用其导出配方。 */
  collection_id?: string;
  /** 「当前筛选结果」：后端展开搜索查询。 */
  query?: string;
  /** 应用导出配方的规格规则（仅 collection_id 存在时生效）。 */
  use_recipe?: boolean;
  sort_by?: string;
  descending?: boolean;
}

export interface ExportProgress {
  current: number;
  total: number;
  filename: string;
}
