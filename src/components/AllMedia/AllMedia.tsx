import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

function AllMedia() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");

  async function greet() {
    const msg = await invoke<string>("greet", { name });
    setGreetMsg(msg);
  }

  return (
    <div className="flex h-full flex-col p-6">
      <h1 className="mb-6 text-2xl font-bold">全部媒体</h1>
      <div className="flex flex-1 flex-col items-center justify-center gap-4 rounded-xl border border-dashed border-neutral-700 bg-neutral-800/50 p-8">
        <p className="text-neutral-400">拖入图片开始导入</p>
        <div className="flex gap-2">
          <input
            value={name}
            onChange={(e) => setName(e.currentTarget.value)}
            placeholder="Enter a name..."
            className="rounded border border-neutral-700 bg-neutral-800 px-3 py-2 text-sm outline-none focus:border-blue-500"
          />
          <button
            type="button"
            onClick={greet}
            className="rounded bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-500"
          >
            Greet
          </button>
        </div>
        {greetMsg && (
          <p className="text-sm text-green-400">{greetMsg}</p>
        )}
      </div>
    </div>
  );
}

export default AllMedia;
