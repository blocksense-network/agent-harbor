/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

export const ChatBox = () => (
  <div class="h-1/5 border-t border-gray-200 p-3">
    <div class="flex h-full flex-col">
      <div class="flex flex-1 flex-col">
        <div class="relative flex-1">
          <div class="mb-1 flex items-center justify-between px-1">
            <div class="flex items-center space-x-1">
              <button
                class={`
                  rounded p-1 text-sm text-gray-500
                  hover:bg-gray-100 hover:text-gray-700
                `}
                title="Add file context"
              >
                📁
              </button>
              <button
                class={`
                  rounded p-1 text-sm text-gray-500
                  hover:bg-gray-100 hover:text-gray-700
                `}
                title="Configure tools"
              >
                🔧
              </button>
              <button
                class={`
                  rounded p-1 text-sm text-gray-500
                  hover:bg-gray-100 hover:text-gray-700
                `}
                title="Add attachments"
              >
                📎
              </button>
            </div>

            <div class="flex items-center space-x-2">
              <div class="flex items-center space-x-1 text-xs text-gray-400">
                <div
                  class="h-2 w-2 rounded-full bg-green-400"
                  title="Context: 2.3K tokens | TPS: 45 | Cost: $0.02"
                />
                <span
                  class={`
                    hidden
                    sm:inline
                  `}
                >
                  2.3K
                </span>
              </div>

              <select
                class={`
                  border-0 bg-transparent text-xs text-gray-500
                  focus:text-gray-700 focus:outline-none
                `}
              >
                <option>GPT-4</option>
                <option>Claude-3</option>
                <option>Gemini Pro</option>
              </select>
            </div>
          </div>

          <div class="flex">
            <textarea
              class={`
                flex-1 resize-none rounded-l border border-gray-300 px-3 py-2
                text-sm
                focus:border-transparent focus:ring-2 focus:ring-blue-500
                focus:outline-none
              `}
              placeholder="Type your message... (Enter to send, Shift+Enter for new line)"
              rows="2"
            />
            <button
              class={`
                rounded-r bg-blue-600 px-4 py-2 text-white
                hover:bg-blue-700
                focus:ring-2 focus:ring-blue-500 focus:outline-none
              `}
            >
              Send
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
);
