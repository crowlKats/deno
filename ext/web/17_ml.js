// Copyright 2018-2025 the Deno authors. MIT license.

import {
  ml_prompt,
  ml_prompt_stream,
  ml_prompt_stream_read,
  ml_prompt_stream_end,
} from "ext:core/ops";

function promptStreaming(prompt) {
  return new ReadableStream({
    async start(controller) {
      const rid = await ml_prompt_stream(prompt);
      let index = 0;

      while (true) {
        const res = await ml_prompt_stream_read(rid, index);
        if (res !== null) {
          if (res.type === "eof") {
            const end = await ml_prompt_stream_end(rid);
            if (end) {
              controller.enqueue(end);
            }
            controller.close();
            break;
          } else if (res.type === "ok") {
            controller.enqueue(res.data);
          }
        }

        if (index >= 10000) {
          const end = await ml_prompt_stream_end(rid);
          if (end) {
            controller.enqueue(end);
          }
          controller.close();
          break;
        }

        index++;
      }
    },
  })
}

export { ml_prompt as prompt, promptStreaming };
