FROM vllm/vllm-openai:latest

RUN pip install redis

COPY . .

CMD ["python", "__main__.py"]