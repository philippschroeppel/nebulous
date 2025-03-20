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
sampling_params = SamplingParams(max_tokens=8192)
llm = LLM(model=model_name)

print(
    f"Listening for messages on Redis stream='{redis_stream}' as consumer='{consumer_name}'..."
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
