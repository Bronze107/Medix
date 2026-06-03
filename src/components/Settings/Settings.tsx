import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { LlamaServerStatus } from "@/types/ai";
import {
  llamaServerStatus,
  llamaServerStart,
  llamaServerStop,
  settingsGetAll,
  settingsSet,
  testProxy,
} from "@/lib/tauri";

type AiMode = "local" | "cloud" | "auto";
type CloudProvider = "claude" | "openai" | "qwen";

function Settings() {
  const [serverStatus, setServerStatus] = useState<LlamaServerStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [starting, setStarting] = useState(false);
  const [stopping, setStopping] = useState(false);

  const [aiMode, setAiMode] = useState<AiMode>("auto");
  const [cloudProvider, setCloudProvider] = useState<CloudProvider>("claude");
  const [apiKey, setApiKey] = useState("");

  // llama.cpp settings
  const [llamaBinPath, setLlamaBinPath] = useState("llama-server");
  const [llamaPort, setLlamaPort] = useState(8080);
  const [llamaModel, setLlamaModel] = useState("");
  const [llamaThreads, setLlamaThreads] = useState(4);
  const [llamaGpuLayers, setLlamaGpuLayers] = useState(0);
  const [llamaCtxSize, setLlamaCtxSize] = useState(4096);
  const [llamaMmproj, setLlamaMmproj] = useState("");
  const [llamaAutoStart, setLlamaAutoStart] = useState(false);
  const [llamaMaxImageDim, setLlamaMaxImageDim] = useState(768);
  const [aiCustomPrompt, setAiCustomPrompt] = useState("");
  const [llamaTemperature, setLlamaTemperature] = useState(0.2);
  const [llamaTopP, setLlamaTopP] = useState(0.9);
  const [llamaMinP, setLlamaMinP] = useState(0.05);
  const [llamaRepeatPenalty, setLlamaRepeatPenalty] = useState(1.05);
  const [llamaMaxTokens, setLlamaMaxTokens] = useState(1024);
  const [semanticThreshold, setSemanticThreshold] = useState(0.25);
  const [searchSemanticEnabled, setSearchSemanticEnabled] = useState(true);
  const [searchFts5Enabled, setSearchFts5Enabled] = useState(true);

  // Image generation API
  const [imageApiProvider, setImageApiProvider] = useState("");
  const [imageApiKey, setImageApiKey] = useState("");
  const [imageApiBaseUrl, setImageApiBaseUrl] = useState("");
  const [imageApiModel, setImageApiModel] = useState("");
  const [imageApiProxy, setImageApiProxy] = useState(""); // legacy, synced with globalProxy
  const [globalProxy, setGlobalProxy] = useState("");
  const [proxyTesting, setProxyTesting] = useState(false);
  const [proxyTestResult, setProxyTestResult] = useState<string | null>(null);
  const [httpPort, setHttpPort] = useState(8765);
  const [showAdvanced, setShowAdvanced] = useState(false);

  const [saved, setSaved] = useState(false);

  const loadSettingsOnce = useCallback(async () => {
    try {
      const settings = await settingsGetAll();
      if (settings.ai_mode) setAiMode(settings.ai_mode as AiMode);
      if (settings.cloud_provider) setCloudProvider(settings.cloud_provider as CloudProvider);
      if (settings.cloud_api_key) setApiKey(settings.cloud_api_key);
      if (settings.llama_bin_path) setLlamaBinPath(settings.llama_bin_path);
      if (settings.llama_port) setLlamaPort(parseInt(settings.llama_port) || 8080);
      if (settings.llama_model) setLlamaModel(settings.llama_model);
      if (settings.llama_threads) setLlamaThreads(parseInt(settings.llama_threads) || 4);
      if (settings.llama_gpu_layers) setLlamaGpuLayers(parseInt(settings.llama_gpu_layers) || 0);
      if (settings.llama_ctx_size) setLlamaCtxSize(parseInt(settings.llama_ctx_size) || 4096);
      if (settings.llama_mmproj) setLlamaMmproj(settings.llama_mmproj);
      if (settings.llama_auto_start) setLlamaAutoStart(settings.llama_auto_start === "true");
      if (settings.llama_max_image_dim) setLlamaMaxImageDim(parseInt(settings.llama_max_image_dim) || 0);
      if (settings.ai_custom_prompt) setAiCustomPrompt(settings.ai_custom_prompt);
      if (settings.llama_temperature) setLlamaTemperature(parseFloat(settings.llama_temperature) || 0.2);
      if (settings.llama_top_p) setLlamaTopP(parseFloat(settings.llama_top_p) || 0.9);
      if (settings.llama_min_p) setLlamaMinP(parseFloat(settings.llama_min_p) || 0.05);
      if (settings.llama_repeat_penalty) setLlamaRepeatPenalty(parseFloat(settings.llama_repeat_penalty) || 1.05);
      if (settings.llama_max_tokens) setLlamaMaxTokens(parseInt(settings.llama_max_tokens) || 1024);
      if (settings.semantic_threshold) setSemanticThreshold(parseFloat(settings.semantic_threshold) || 0.25);
      if (settings.search_semantic_enabled) setSearchSemanticEnabled(settings.search_semantic_enabled === "true");
      if (settings.search_fts5_enabled) setSearchFts5Enabled(settings.search_fts5_enabled === "true");
      if (settings.http_port) setHttpPort(parseInt(settings.http_port) || 8765);
      if (settings.image_api_provider) setImageApiProvider(settings.image_api_provider);
      if (settings.image_api_key) setImageApiKey(settings.image_api_key);
      if (settings.image_api_base_url) setImageApiBaseUrl(settings.image_api_base_url);
      if (settings.image_api_model) setImageApiModel(settings.image_api_model);
      if (settings.image_api_proxy) setImageApiProxy(settings.image_api_proxy);
      if (settings.global_proxy) {
        setGlobalProxy(settings.global_proxy);
      } else if (settings.image_api_proxy) {
        setGlobalProxy(settings.image_api_proxy); // migrate from legacy key
      }
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }, []);

  const pollStatus = useCallback(async () => {
    try {
      const status = await llamaServerStatus();
      setServerStatus(status);
    } catch (e) {
      console.error("Failed to poll status:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSettingsOnce();
    pollStatus();
    const interval = setInterval(pollStatus, 5000);
    return () => clearInterval(interval);
  }, [loadSettingsOnce, pollStatus]);

  const handleSave = async () => {
    try {
      await settingsSet("ai_mode", aiMode);
      await settingsSet("cloud_provider", cloudProvider);
      await settingsSet("cloud_api_key", apiKey);
      await settingsSet("llama_bin_path", llamaBinPath);
      await settingsSet("llama_port", String(llamaPort));
      await settingsSet("llama_model", llamaModel);
      await settingsSet("llama_threads", String(llamaThreads));
      await settingsSet("llama_gpu_layers", String(llamaGpuLayers));
      await settingsSet("llama_ctx_size", String(llamaCtxSize));
      await settingsSet("llama_mmproj", llamaMmproj);
      await settingsSet("llama_auto_start", llamaAutoStart ? "true" : "false");
      await settingsSet("llama_max_image_dim", String(llamaMaxImageDim));
      await settingsSet("ai_custom_prompt", aiCustomPrompt);
      await settingsSet("llama_temperature", String(llamaTemperature));
      await settingsSet("llama_top_p", String(llamaTopP));
      await settingsSet("llama_min_p", String(llamaMinP));
      await settingsSet("llama_repeat_penalty", String(llamaRepeatPenalty));
      await settingsSet("llama_max_tokens", String(llamaMaxTokens));
      await settingsSet("semantic_threshold", String(semanticThreshold));
      await settingsSet("search_semantic_enabled", searchSemanticEnabled ? "true" : "false");
      await settingsSet("search_fts5_enabled", searchFts5Enabled ? "true" : "false");
      await settingsSet("http_port", String(httpPort));
      await settingsSet("image_api_provider", imageApiProvider);
      await settingsSet("image_api_key", imageApiKey);
      await settingsSet("image_api_base_url", imageApiBaseUrl);
      await settingsSet("image_api_model", imageApiModel);
      await settingsSet("image_api_proxy", imageApiProxy);
      await settingsSet("global_proxy", globalProxy);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  };

  const handleStart = async () => {
    setStarting(true);
    try {
      await handleSave();
      await llamaServerStart();
      await pollStatus();
    } catch (e) {
      console.error("Failed to start server:", e);
      alert(`启动失败: ${e}`);
    } finally {
      setStarting(false);
    }
  };

  const handleStop = async () => {
    setStopping(true);
    try {
      await llamaServerStop();
      await pollStatus();
    } catch (e) {
      console.error("Failed to stop server:", e);
    } finally {
      setStopping(false);
    }
  };

  const showCloudSettings = aiMode === "cloud" || aiMode === "auto";

  return (
    <div className="flex h-full flex-col p-6">
      <h1 className="mb-6 text-2xl font-bold">设置</h1>

      <div className="flex-1 space-y-6 overflow-auto">
        {/* AI Mode */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            AI 推理模式
          </h2>
          <div className="space-y-2">
            {(
              [
                { value: "auto" as AiMode, label: "自动 (Auto)", desc: "优先本地，不可用时切换云端" },
                { value: "local" as AiMode, label: "本地 (Local)", desc: "仅使用本地 llama.cpp" },
                { value: "cloud" as AiMode, label: "云端 (Cloud)", desc: "仅使用云端 API" },
              ] as const
            ).map((opt) => (
              <label
                key={opt.value}
                className="flex cursor-pointer items-start gap-3 rounded border border-transparent p-2 hover:bg-[var(--color-bg-tertiary)]"
              >
                <input
                  type="radio"
                  name="ai_mode"
                  value={opt.value}
                  checked={aiMode === opt.value}
                  onChange={(e) => setAiMode(e.target.value as AiMode)}
                  className="mt-0.5"
                />
                <div>
                  <p className="text-sm text-[var(--color-text-secondary)]">{opt.label}</p>
                  <p className="text-xs text-[var(--color-text-muted)]">{opt.desc}</p>
                </div>
              </label>
            ))}
          </div>
        </section>

        {/* Custom Prompt */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-sm font-semibold text-[var(--color-text-primary)]">
              自定义提示词
            </h2>
            <button
              onClick={() => setAiCustomPrompt("")}
              className="rounded px-2 py-0.5 text-[11px] text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)] transition-colors"
            >
              恢复默认
            </button>
          </div>
          <textarea
            value={aiCustomPrompt}
            onChange={(e) => setAiCustomPrompt(e.target.value)}
            rows={6}
            placeholder={`Describe this image in detail. Then on a new line starting with "TAGS:", list key objects, concepts, and visual elements as comma-separated lowercase tags.

Example output:
A golden retriever playing fetch with a red ball in a sunny park with green grass and trees.
TAGS: dog, golden retriever, ball, park, grass, trees, outdoor, sunny`}
            className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-2 text-xs text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)] focus:border-[var(--color-accent)] resize-y"
          />
          <p className="mt-1 text-[11px] text-[var(--color-text-muted)]">
            留空使用默认英文提示。支持自定义语言、风格和输出格式
          </p>
        </section>

        {/* llama.cpp Server */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            llama.cpp 本地服务
          </h2>

          {/* Status */}
          <div className="mb-4 flex items-center gap-3">
            {loading && !serverStatus ? (
              <span className="text-sm text-[var(--color-text-muted)]">检测中...</span>
            ) : serverStatus?.running ? (
              <>
                <span className="h-2 w-2 rounded-full bg-[var(--color-success)]"></span>
                <span className="text-sm text-[var(--color-success)]">
                  运行中 (PID: {serverStatus.pid}, 端口: {serverStatus.port})
                </span>
              </>
            ) : (
              <>
                <span className="h-2 w-2 rounded-full bg-[var(--color-danger)]"></span>
                <span className="text-sm text-[var(--color-danger)]">已停止</span>
              </>
            )}
          </div>

          {/* Start/Stop buttons */}
          <div className="mb-4 flex gap-2">
            <button
              onClick={handleStart}
              disabled={serverStatus?.running || starting}
              className="rounded bg-[var(--color-success)] px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-[var(--color-success)]/80 disabled:opacity-50"
            >
              {starting ? "启动中..." : "启动服务"}
            </button>
            <button
              onClick={handleStop}
              disabled={!serverStatus?.running || stopping}
              className="rounded border border-[var(--color-danger)]/20 bg-[var(--color-danger-soft)] px-3 py-1.5 text-xs font-medium text-[var(--color-danger)] transition-colors hover:bg-[var(--color-danger-soft)]/80 disabled:opacity-50"
            >
              {stopping ? "停止中..." : "停止服务"}
            </button>
          </div>

          {/* Auto-start checkbox */}
          <label className="mb-4 flex cursor-pointer items-center gap-2">
            <input
              type="checkbox"
              checked={llamaAutoStart}
              onChange={(e) => setLlamaAutoStart(e.target.checked)}
              className="h-3.5 w-3.5"
            />
            <span className="text-xs text-[var(--color-text-secondary)]">
              应用启动时自动启动 llama-server
            </span>
          </label>

          {/* Advanced settings toggle */}
          <button
            onClick={() => setShowAdvanced((v) => !v)}
            className="mb-3 flex items-center gap-1 text-xs text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className={`h-3 w-3 transition-transform ${showAdvanced ? "rotate-90" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="m8.25 4.5 7.5 7.5-7.5 7.5" />
            </svg>
            高级设置
          </button>

          {showAdvanced && (
            <>
              {/* Port */}
              <div className="mb-3">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">端口号</label>
                <input
                  type="number"
                  value={llamaPort}
                  onChange={(e) => setLlamaPort(parseInt(e.target.value) || 8080)}
                  className="w-28 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
                />
              </div>

              {/* Binary path */}
              <div className="mb-3">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                  llama-server 二进制路径
                </label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    value={llamaBinPath}
                    onChange={(e) => setLlamaBinPath(e.target.value)}
                    placeholder="C:\path\to\llama-server.exe"
                    className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
                  />
                  <button
                    onClick={async () => {
                      const path = await open({
                        filters: [{ name: "可执行文件", extensions: ["exe"] }],
                      });
                      if (path) setLlamaBinPath(path);
                    }}
                    className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
                    title="选择可执行文件"
                  >
                    <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
                    </svg>
                  </button>
                </div>
              </div>

              {/* Threads */}
              <div className="mb-3">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                  线程数: {llamaThreads}
                </label>
                <input
                  type="range"
                  min={1}
                  max={16}
                  value={llamaThreads}
                  onChange={(e) => setLlamaThreads(parseInt(e.target.value))}
                  className="w-48"
                />
              </div>

              {/* GPU Layers */}
              <div className="mb-3">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                  GPU 层数 (0 = 纯CPU): {llamaGpuLayers}
                </label>
                <input
                  type="range"
                  min={0}
                  max={99}
                  step={1}
                  value={llamaGpuLayers}
                  onChange={(e) => setLlamaGpuLayers(parseInt(e.target.value))}
                  className="w-48"
                />
              </div>

              {/* Context Size */}
              <div className="mb-3">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                  上下文大小: {llamaCtxSize}
                </label>
                <input
                  type="number"
                  value={llamaCtxSize}
                  onChange={(e) => setLlamaCtxSize(parseInt(e.target.value) || 4096)}
                  step={512}
                  className="w-28 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
                />
              </div>

              {/* Max Image Dimension */}
              <div className="mb-3">
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
                  推理分辨率: {llamaMaxImageDim === 0 ? "不缩放" : `${llamaMaxImageDim}px`}
                </label>
                <input
                  type="range"
                  min={0}
                  max={2048}
                  step={64}
                  value={llamaMaxImageDim}
                  onChange={(e) => setLlamaMaxImageDim(parseInt(e.target.value))}
                  className="w-48"
                />
                <p className="mt-0.5 text-[11px] text-[var(--color-text-muted)]">
                  限制发送给 VLM 的图片长边尺寸，0 = 不缩放。降低可加快推理，推荐 768~1024
                </p>
              </div>

              {/* Sampling Parameters */}
              <div className="mb-3 border-t border-[var(--color-border-light)] pt-3">
                <p className="mb-2 text-xs font-medium text-[var(--color-text-primary)]">采样参数</p>

                {/* Temperature */}
                <div className="mb-2">
                  <label className="mb-0.5 block text-[11px] text-[var(--color-text-muted)]">
                    Temperature: {llamaTemperature.toFixed(2)}
                  </label>
                  <input type="range" min={0} max={2} step={0.05} value={llamaTemperature}
                    onChange={(e) => setLlamaTemperature(parseFloat(e.target.value))} className="w-48" />
                  <p className="text-[11px] text-[var(--color-text-muted)]">控制输出随机性。低值更一致，高值更多样。标注任务推荐 0.1~0.3</p>
                </div>

                {/* Top-P */}
                <div className="mb-2">
                  <label className="mb-0.5 block text-[11px] text-[var(--color-text-muted)]">
                    Top-P: {llamaTopP.toFixed(2)}
                  </label>
                  <input type="range" min={0} max={1} step={0.05} value={llamaTopP}
                    onChange={(e) => setLlamaTopP(parseFloat(e.target.value))} className="w-48" />
                  <p className="text-[11px] text-[var(--color-text-muted)]">Nucleus 采样阈值，过滤低概率 token。推荐 0.85~0.95</p>
                </div>

                {/* Min-P */}
                <div className="mb-2">
                  <label className="mb-0.5 block text-[11px] text-[var(--color-text-muted)]">
                    Min-P: {llamaMinP.toFixed(2)}
                  </label>
                  <input type="range" min={0} max={0.3} step={0.01} value={llamaMinP}
                    onChange={(e) => setLlamaMinP(parseFloat(e.target.value))} className="w-48" />
                  <p className="text-[11px] text-[var(--color-text-muted)]">最小概率阈值，砍掉低于此值的 token。推荐 0.02~0.1</p>
                </div>

                {/* Repeat Penalty */}
                <div className="mb-2">
                  <label className="mb-0.5 block text-[11px] text-[var(--color-text-muted)]">
                    Repeat Penalty: {llamaRepeatPenalty.toFixed(2)}
                  </label>
                  <input type="range" min={1} max={1.5} step={0.05} value={llamaRepeatPenalty}
                    onChange={(e) => setLlamaRepeatPenalty(parseFloat(e.target.value))} className="w-48" />
                  <p className="text-[11px] text-[var(--color-text-muted)]">重复惩罚，{'>'}1.0 降低已出现 token 的概率。推荐 1.0~1.15</p>
                </div>

                {/* Max Tokens */}
                <div>
                  <label className="mb-0.5 block text-[11px] text-[var(--color-text-muted)]">
                    Max Tokens: {llamaMaxTokens}
                  </label>
                  <input type="number" min={64} max={4096} step={64} value={llamaMaxTokens}
                    onChange={(e) => setLlamaMaxTokens(parseInt(e.target.value) || 1024)}
                    className="w-28 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1 text-xs text-[var(--color-text-primary)] outline-none" />
                  <p className="mt-0.5 text-[11px] text-[var(--color-text-muted)]">最大输出 token 数。caption+tags 通常 150~300 token 足够</p>
                </div>
              </div>
            </>
          )}

        </section>

        {/* Global Proxy */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            网络代理
          </h2>
          <div className="flex items-end gap-2">
            <div className="flex-1">
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">HTTP 代理地址（可选）</label>
              <input
                type="text"
                value={globalProxy}
                onChange={(e) => { setGlobalProxy(e.target.value); setProxyTestResult(null); }}
                placeholder="http://127.0.0.1:7890"
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
            </div>
            <button
              onClick={async () => {
                if (!globalProxy.trim()) return;
                setProxyTesting(true);
                setProxyTestResult(null);
                try {
                  const msg = await testProxy(globalProxy.trim());
                  setProxyTestResult(msg);
                } catch (e) {
                  setProxyTestResult(String(e));
                } finally {
                  setProxyTesting(false);
                }
              }}
              disabled={proxyTesting || !globalProxy.trim()}
              className="shrink-0 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-3 py-1.5 text-xs text-[var(--color-text-secondary)] transition-colors hover:bg-[var(--color-bg-hover)] disabled:opacity-50 active:scale-[0.97]"
            >
              {proxyTesting ? "测试中..." : "测试连接"}
            </button>
          </div>
          {proxyTestResult && (
            <p className={`mt-1.5 text-xs ${proxyTestResult.startsWith("连接成功") ? "text-[var(--color-success)]" : "text-[var(--color-danger)]"}`}>
              {proxyTestResult}
            </p>
          )}
          <p className="mt-1 text-[11px] text-[var(--color-text-muted)]">
            所有外部 HTTP 请求（AI 生图、浏览器插件下载图片）均通过此代理。留空则尝试读取 HTTPS_PROXY 环境变量
          </p>
        </section>

        {/* Browser Extension */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            浏览器插件
          </h2>
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              HTTP 服务端口
            </label>
            <input
              type="number"
              value={httpPort}
              onChange={(e) => setHttpPort(parseInt(e.target.value) || 8765)}
              className="w-28 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
            />
            <p className="mt-0.5 text-[11px] text-[var(--color-text-muted)]">
              浏览器插件通过此端口与 Medix 通信，修改后需重启应用
            </p>
          </div>
        </section>

        {/* Search Settings */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            搜索
          </h2>

          {/* Semantic search toggle */}
          <label className="mb-3 flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={searchSemanticEnabled}
              onChange={(e) => setSearchSemanticEnabled(e.target.checked)}
              className="h-4 w-4 rounded accent-[var(--color-accent)]"
            />
            <div>
              <span className="text-xs text-[var(--color-text-primary)]">语义搜索</span>
              <p className="text-[11px] text-[var(--color-text-muted)]">
                基于 embedding 向量相似度匹配，支持自然语言描述搜索
              </p>
            </div>
          </label>

          {/* FTS5 toggle */}
          <label className="mb-3 flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={searchFts5Enabled}
              onChange={(e) => setSearchFts5Enabled(e.target.checked)}
              className="h-4 w-4 rounded accent-[var(--color-accent)]"
            />
            <div>
              <span className="text-xs text-[var(--color-text-primary)]">FTS5 全文搜索</span>
              <p className="text-[11px] text-[var(--color-text-muted)]">
                基于 SQLite FTS5 的精确文本匹配，索引所有 caption 和标签
              </p>
            </div>
          </label>

          {/* Threshold slider */}
          <div>
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              语义相似度阈值: {semanticThreshold.toFixed(2)}
            </label>
            <input
              type="range"
              min={0}
              max={0.5}
              step={0.01}
              value={semanticThreshold}
              onChange={(e) => setSemanticThreshold(parseFloat(e.target.value))}
              className="w-48"
            />
            <p className="mt-0.5 text-[11px] text-[var(--color-text-muted)]">
              越高越严格（只返回高度相关的图片），越低越宽松。默认 0.25
            </p>
          </div>
        </section>

        {/* GGUF Models */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            GGUF 模型
          </h2>

          {/* Model path */}
          <div className="mb-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">模型文件路径</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={llamaModel}
                onChange={(e) => setLlamaModel(e.target.value)}
                placeholder="C:\models\model.gguf"
                className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
              <button
                onClick={async () => {
                  const path = await open({
                    filters: [{ name: "GGUF 模型", extensions: ["gguf"] }],
                  });
                  if (path) setLlamaModel(path);
                }}
                className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
                title="选择模型文件"
              >
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
                </svg>
              </button>
            </div>
            <p className="mt-0.5 text-[11px] text-[var(--color-text-muted)]">
              输入 .gguf 模型文件的完整路径，或点击浏览选择文件
            </p>
          </div>

          {/* mmproj (vision projector) */}
          <div className="mb-3 border-t border-[var(--color-border)] pt-3">
            <label className="mb-1 block text-xs text-[var(--color-text-muted)]">
              mmproj 视觉投影器
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={llamaMmproj}
                onChange={(e) => setLlamaMmproj(e.target.value)}
                placeholder="留空表示不使用 VLM 视觉功能"
                className="flex-1 rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
              <button
                onClick={async () => {
                  const path = await open({
                    filters: [{ name: "mmproj 文件", extensions: ["gguf"] }],
                  });
                  if (path) setLlamaMmproj(path);
                }}
                className="rounded-lg p-2 text-[var(--color-text-muted)] hover:bg-[var(--color-bg-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
                title="选择 mmproj 文件"
              >
                <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
                </svg>
              </button>
            </div>
            <p className="mt-0.5 text-[11px] text-[var(--color-text-muted)]">
              使用 VLM 模型时需要配套的 mmproj 文件
            </p>
          </div>
        </section>

        {/* Cloud Settings */}
        {showCloudSettings && (
          <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
            <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
              云端 API 配置
            </h2>
            <div className="space-y-3">
              <div>
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">服务商</label>
                <select
                  value={cloudProvider}
                  onChange={(e) => setCloudProvider(e.target.value as CloudProvider)}
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
                >
                  <option value="claude">Claude (Anthropic)</option>
                  <option value="openai">OpenAI</option>
                  <option value="qwen">Qwen (阿里云)</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-xs text-[var(--color-text-muted)]">API Key</label>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
                />
              </div>
            </div>
          </section>
        )}

        {/* Image Generation API */}
        <section className="rounded-lg border border-[var(--color-border)] bg-[var(--color-bg-secondary)] p-4">
          <h2 className="mb-3 text-sm font-semibold text-[var(--color-text-primary)]">
            🖼 图像生成 API
          </h2>
          <div className="space-y-3">
            <div>
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">服务商</label>
              <select
                value={imageApiProvider}
                onChange={(e) => setImageApiProvider(e.target.value)}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none"
              >
                <option value="">未配置</option>
                <option value="xai">xAI (Grok Imagine)</option>
              </select>
            </div>
            <div>
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">API Key</label>
              <input
                type="password"
                value={imageApiKey}
                onChange={(e) => setImageApiKey(e.target.value)}
                placeholder="xai-..."
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">服务地址</label>
              <input
                type="text"
                value={imageApiBaseUrl}
                onChange={(e) => setImageApiBaseUrl(e.target.value)}
                placeholder={imageApiProvider === "xai" ? "https://api.x.ai/v1" : ""}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
            </div>
            <div>
              <label className="mb-1 block text-xs text-[var(--color-text-muted)]">模型</label>
              <input
                type="text"
                value={imageApiModel}
                onChange={(e) => setImageApiModel(e.target.value)}
                placeholder={imageApiProvider === "xai" ? "grok-imagine-image-quality" : ""}
                className="w-full rounded border border-[var(--color-border-light)] bg-[var(--color-bg-tertiary)] px-2 py-1.5 text-sm text-[var(--color-text-primary)] outline-none placeholder:text-[var(--color-text-muted)]"
              />
            </div>
          </div>
        </section>

      </div>

      {/* Floating save bar */}
      <div className="border-t border-[var(--color-border)] bg-[var(--color-bg-primary)] pt-3 flex items-center gap-3">
        <button
          onClick={handleSave}
          className="rounded bg-[var(--color-accent)] px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-[var(--color-accent-hover)]"
        >
          保存设置
        </button>
        {saved && <span className="text-sm text-[var(--color-success)]">已保存</span>}
      </div>
    </div>
  );
}

export default Settings;
