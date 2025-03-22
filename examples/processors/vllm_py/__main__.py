# pylint: disable=all
# flake8: noqa
# type: ignore

import json
import os
import time

import redis
from vllm import LLM, SamplingParams

####
# These redis env vars are always set on any processor
####

redis_url = os.getenv("REDIS_URL")
if not redis_url:
    raise ValueError("REDIS_URL is not set")

redis_stream = os.getenv("REDIS_STREAM")
if not redis_stream:
    raise ValueError("REDIS_STREAM is not set")

redis_consumer_group = os.getenv("REDIS_CONSUMER_GROUP")
if not redis_consumer_group:
    raise ValueError("REDIS_CONSUMER_GROUP is not set")

model_name = os.getenv("MODEL")
if not model_name:
    raise ValueError("MODEL is not set")

print(
    f"using env vars -> url: {redis_url}, stream: {redis_stream}, group: {redis_consumer_group}, model: {model_name}"
)

print(f"Connecting to Redis...")
r = redis.Redis.from_url(url=redis_url, db=0)
print(f"Connected to Redis.")

# Create consumer group if it doesn't exist
try:
    r.xgroup_create(
        name=redis_stream, groupname=redis_consumer_group, id="0", mkstream=True
    )
    print(
        f"Created consumer group '{redis_consumer_group}' on stream '{redis_stream}'."
    )
except redis.exceptions.ResponseError as e:
    # If it's BUSYGROUP, group already exists, so it's safe to ignore.
    if "BUSYGROUP" in str(e):
        pass
    else:
        raise e

# We'll generate a new consumer name for each run
consumer_name = f"consumer-{time.time()}"

# Configure model and sampling parameters
llm = LLM(model=model_name)

print(
    f"Listening for messages on Redis stream='{redis_stream}' as consumer='{consumer_name}'..."
)


def get_sampling_params(params: dict) -> SamplingParams:
    """
    Convert OpenAI-style parameters to vllm SamplingParams.
    If a parameter has no direct equivalent in vllm, we skip it or set defaults.
    """

    # From OpenAI params
    temperature = params.get("temperature", 1.0)
    top_p = params.get("top_p", 1.0)
    n = params.get("n", 1)

    # 'max_completion_tokens' takes precedence over 'max_tokens'
    max_tokens = params.get("max_completion_tokens")
    if max_tokens is None:
        max_tokens = params.get("max_tokens", 256)  # default if not provided

    presence_penalty = params.get("presence_penalty", 0.0)
    frequency_penalty = params.get("frequency_penalty", 0.0)
    stop = params.get("stop", None)
    # Could be str or list of str in OpenAI. vllm also accepts Optional[List[str]].

    seed = params.get("seed", None)
    # vllm seeds are experimental/don't guarantee perfectly reproducible outputs,
    # but it's included here since OpenAI exposes it.

    # OpenAI allows logprobs=bool and top_logprobs=int
    # vllm supports logprobs=bool. We'll also pass top_logprobs if present.
    logprobs = params.get("logprobs", False)
    top_logprobs = params.get("top_logprobs", None)

    # The 'logit_bias' from OpenAI is a Dict[str, int], mapping token IDs to bias
    logit_bias = params.get("logit_bias", None)

    # vllm does not have an exact equivalent for 'service_tier', 'user', 'store', etc.
    # We'll skip them here, or you can handle them separately if needed.

    # We'll read some advanced/optional vllm parameters from the snippet
    # if they exist in the OpenAI request. Otherwise, we default them.
    repetition_penalty = 1.0  # default in vllm snippet
    if "repetition_penalty" in params:
        # If you want to allow it from some extended param, do so here.
        repetition_penalty = params["repetition_penalty"]

    top_k = None
    if "top_k" in params:
        top_k = params["top_k"]

    min_p = None
    if "min_p" in params:
        min_p = params["min_p"]

    # If you'd like to parse more advanced fields, do so similarly.
    # Example: "best_of", "stop_token_ids", etc.

    # Some advanced text-generation features
    ignore_eos = False
    if "ignore_eos" in params:
        ignore_eos = params["ignore_eos"]

    # Response format ( JSON / plain text / etc. )
    output_kind = None
    response_format = params.get("response_format")
    if response_format and isinstance(response_format, dict):
        # e.g. { "type": "json_object" } or { "type": "json_schema", ... }
        if response_format.get("type") == "json_object":
            output_kind = "json"
        # If it’s "json_schema", you might also set output_kind="json"
        # or treat it differently if your model needs special handling.

    return SamplingParams(
        n=n,
        presence_penalty=presence_penalty,
        frequency_penalty=frequency_penalty,
        repetition_penalty=repetition_penalty,
        temperature=temperature,
        top_p=top_p,
        top_k=top_k,
        min_p=min_p,
        seed=seed,
        stop=stop,
        max_tokens=max_tokens,
        logprobs=logprobs,
        top_logprobs=top_logprobs,
        logit_bias=logit_bias,
        ignore_eos=ignore_eos,
        output_kind=output_kind,
    )


while True:
    try:
        # Read from the consumer group (block for up to 5000 ms if there's no data)
        entries = r.xreadgroup(
            groupname=redis_consumer_group,
            consumername=consumer_name,
            streams={redis_stream: ">"},
            count=1,
            block=5000,
        )
        print(f"Read {len(entries)} entries from Redis.")

        if not entries:
            # No new messages in the given period
            time.sleep(0.1)
            continue

        # entries will look like: [(b'stream_name', [(b'entry_id', {b'field': b'value', ...}), ...])]
        stream_name, messages = entries[0]
        print(f"Messages: {messages}")
        for entry_id, fields in messages:
            # The fields dictionary contains your message data (binary-encoded)
            # e.g. fields might be {'data': '...some json...'}
            raw_data = fields.get(b"data", b"").decode("utf-8", errors="replace")
            print(f"Received message from Redis (id={entry_id}): {raw_data}")

            try:
                # Parse the openai-format messages from JSON
                openai_messages = json.loads(raw_data)

                print(f"OpenAI Messages: {openai_messages}")

                sampling_params = get_sampling_params(openai_messages)
                # Invoke the model's chat method
                outputs = llm.chat(openai_messages, sampling_params=sampling_params)
                print(f"Outputs: {outputs}")
                # Print or handle the model's output in your own way
                # Here we assume a single-completion output
                print("Model Response:")
                print(outputs[0].outputs[0].text.strip())

            except Exception as e:
                print(f"Error processing message from Redis (id={entry_id}): {e}")

            finally:
                # Acknowledge that we've processed the message
                print(f"Acknowledging message (id={entry_id})")
                r.xack(redis_stream, redis_consumer_group, entry_id)

    except Exception as e:
        print(f"Error reading from Redis streams: {e}")
        # Continue looping in case of transient error
        time.sleep(0.1)
