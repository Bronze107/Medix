import { useCallback, useEffect, useState } from "react";
import type { OllamaStatus, ModelStatus } from "@/types/ai";
import {
  ollamaStatus,
  modelList,
  settingsGetAll,
  settingsSet,
} from "@/lib/tauri";

type AiMode = "local" | "cloud" | "auto";
type CloudProvider = "claude" | "openai" | "qwen";

function Settings() {
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [loading, setLoading] = useState(true);

  const [aiMode, setAiMode] = useState<AiMode>("auto");
  const [cloudProvider, setCloudProvider] = useState<CloudProvider>("claude");
  const [apiKey, setApiKey] = useState("");
  const [saved, setSaved] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const [status, mlist, settings] = await Promise.all([
        ollamaStatus(),
        modelList(),
        settingsGetAll(),
      ]);
      setOllama(status);

      // Merge ollama model info into model status
      const merged = mlist.map((m) => {
        const installed = status.models.some((om) =>
          om.name.toLowerCase().startsWith(m.name.toLowerCase().replace(" ", ""))
        );
        return { ...m, installed };
      });
      setModels(merged);

      if (settings.ai_mode) setAiMode(settings.ai_mode as AiMode);
      if (settings.cloud_provider)
        setCloudProvider(settings.cloud_provider as CloudProvider);
      if (settings.cloud_api_key) setApiKey(settings.cloud_api_key);
    } catch (e) {
      console.error("Failed to load settings:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 5000);
    return () => clearInterval(interval);
  }, [loadData]);

  const handleSave = async () => {
    try {
      await settingsSet("ai_mode", aiMode);
      await settingsSet("cloud_provider", cloudProvider);
      await settingsSet("cloud_api_key", apiKey);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("Failed to save settings:", e);
    }
  };

  const showCloudSettings = aiMode === "cloud" || aiMode === "auto";

  return (
    <div className="flex h-full flex-col p-6">
      <h1 className="mb-6 text-2xl font-bold">设置</h1>

      <div className="flex-1 space-y-6 overflow-auto">
        {/* AI Mode */}
        <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <h2 className="mb-3 text-sm font-semibold text-neutral-200">
            AI 推理模式
          </h2>
          <div className="space-y-2">
            {(
              [
                { value: "auto" as AiMode, label: "自动 (Auto)", desc: "优先本地，不可用时降级云端" },
                { value: "local" as AiMode, label: "本地 (Local)", desc: "仅使用本地 Ollama 模型" },
                { value: "cloud" as AiMode, label: "云端 (Cloud)", desc: "仅使用云端 API" },
              ] as const
            ).map((opt) => (
              <label
                key={opt.value}
                className="flex cursor-pointer items-start gap-3 rounded border border-transparent p-2 hover:bg-neutral-800">
                <input
                  type="radio"
                  name="ai_mode"
                  value={opt.value}
                  checked={aiMode === opt.value}
                  onChange={(e) => setAiMode(e.target.value as AiMode)}
                  className="mt-0.5"
                />
                <div>
                  <p className="text-sm text-neutral-300">{opt.label}</p>
                  <p className="text-xs text-neutral-500">{opt.desc}</p>
                </div>
              </label>
            ))}
          </div>
        </section>

        {/* Ollama Status */}
        <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <h2 className="mb-3 text-sm font-semibold text-neutral-200">
            Ollama 本地服务
          </h2>
          {loading && !ollama ? (
            <p className="text-sm text-neutral-500">检测中...</p>
          ) : ollama?.running ? (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="h-2 w-2 rounded-full bg-green-500"></span>
                <span className="text-sm text-green-400">运行中</span>
                <span className="text-xs text-neutral-500">
                  v{ollama.version}
                </span>
              </div>
              <div className="mt-2">
                <p className="mb-1 text-xs text-neutral-500">已安装模型</p>
                <div className="flex flex-wrap gap-2">
                  {ollama.models.map((m) => (
                    <span
                      key={m.digest}
                      className="rounded bg-neutral-800 px-2 py-0.5 text-xs text-neutral-300"
                    >
                      {m.name}
                    </span>
                  ))}
                  {ollama.models.length === 0 && (
                    <span className="text-xs text-neutral-600">暂无模型</span>
                  )}
                </div>
              </div>
            </div>
          ) : (
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="h-2 w-2 rounded-full bg-red-500"></span>
                <span className="text-sm text-red-400">未运行</span>
              </div>
              <p className="text-xs text-neutral-500">
                请先安装并启动 Ollama，然后拉取所需模型：
              </p>
              <pre className="mt-1 rounded bg-neutral-800 p-2 text-xs text-neutral-400">
                ollama pull minicpm-v{"\n"}
                ollama pull nomic-embed-text
              </pre>
            </div>
          )}
        </section>

        {/* Model Status */}
        <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
          <h2 className="mb-3 text-sm font-semibold text-neutral-200">
            所需模型
          </h2>
          <div className="space-y-2">
            {models.map((m) => (
              <div
                key={m.name}
                className="flex items-center justify-between rounded bg-neutral-800/50 p-2"
              >
                <div>
                  <p className="text-sm text-neutral-300">{m.name}</p>
                  <p className="text-xs text-neutral-500">{Math.round(m.size_mb)} MB</p>
                </div>
                {m.installed ? (
                  <span className="rounded bg-green-900/30 px-2 py-0.5 text-xs text-green-400">
                    已安装
                  </span>
                ) : (
                  <span className="rounded bg-neutral-800 px-2 py-0.5 text-xs text-neutral-500">
                    未安装
                  </span>
                )}
              </div>
            ))}
          </div>
        </section>

        {/* Cloud Settings */}
        {showCloudSettings && (
          <section className="rounded-lg border border-neutral-800 bg-neutral-900 p-4">
            <h2 className="mb-3 text-sm font-semibold text-neutral-200">
              云端 API 配置
            </h2>
            <div className="space-y-3">
              <div>
                <label className="mb-1 block text-xs text-neutral-500">服务商</label>
                <select
                  value={cloudProvider}
                  onChange={(e) =>
                    setCloudProvider(e.target.value as CloudProvider)
                  }
                  className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none"
                >
                  <option value="claude">Claude (Anthropic)</option>
                  <option value="openai">OpenAI</option>
                  <option value="qwen">Qwen (阿里云)</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-xs text-neutral-500">API Key</label>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-..."
                  className="w-full rounded border border-neutral-700 bg-neutral-800 px-2 py-1.5 text-sm text-neutral-200 outline-none placeholder:text-neutral-600"
                />
              </div>
            </div>
          </section>
        )}

        <div className="flex items-center gap-3 pt-2">
          <button
            onClick={handleSave}
            className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
          >
            保存设置
          </button>
          {saved && (
            <span className="text-sm text-green-400">已保存</span>
          )}
        </div>
      </div>
    </div>
  );
}

export default Settings;
