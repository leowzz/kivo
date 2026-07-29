import telLayout from "../models/prod/tel001.json";
import type { AppSnapshot, ModelConfig } from "./types";

const model: ModelConfig = {
  schema_version: 1,
  model: telLayout,
  hardware: {
    controller: "esp32s3",
    debounce_ms: 30,
    inputs: [
      { type: "direct", id: "function-keys", keys: { UP: 6, DOWN: 7, DEL: 8 } },
      {
        type: "contact_matrix",
        id: "carbon-keypad",
        pins: [1, 2, 3, 12, 13, 14, 15],
        keys: {
          DIGIT_1: [1, 12], DIGIT_2: [1, 13], DIGIT_3: [1, 14],
          DIGIT_4: [2, 12], DIGIT_5: [2, 13], DIGIT_6: [2, 14],
          DIGIT_7: [3, 12], DIGIT_8: [3, 13], DIGIT_9: [3, 14],
          STAR: [1, 15], DIGIT_0: [2, 15], HASH: [3, 15],
        },
      },
    ],
  },
  actions: {
    DIGIT_2: [{ type: "paste", text: "你好" }, { type: "hotkey", keys: ["enter"] }],
    SPEAKER: [{ type: "hotkey", keys: ["cmd", "shift", "k"] }],
  },
};

export const previewSnapshot: AppSnapshot = {
  models: [model],
  activeModel: model.model.id,
  language: "zh-CN",
  supportedGpios: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18],
  connection: { state: "searching", port: null },
  runtimeError: null,
  learning: null,
};
