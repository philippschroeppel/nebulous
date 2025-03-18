{{- define "common.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
app.kubernetes.io/name: {{ .Chart.Name | trunc 63 | trimSuffix "-" }}
app.kubernetes.io/instance: {{ .Release.Name | trunc 63 | trimSuffix "-" }}
{{- end}}

{{- define "nebulous.namespace" -}}
{{- if .Values.namespaceOverride }}
{{- .Values.namespaceOverride }}
{{- else }}
{{- .Release.Namespace }}
{{- end }}
{{- end }}

{{- define "nebulous.serviceAccountName" -}}
{{- default .Release.Name .Values.serviceAccount.name }}
{{- end }}

{{- define "nebulous.serviceName" -}}
{{- default .Release.Name .Values.service.name }}
{{- end }}

{{- define "nebulous.servicePort" -}}
{{- default 3000 .Values.service.port }}
{{- end }}

{{- define "nebulous.image" -}}
{{- $tag := default .Chart.AppVersion .Values.image.tag }}
{{- $repository := default "us-docker.pkg.dev/agentsea-dev/nebulous/server" .Values.image.repository }}
{{- printf "%s:%s" $repository $tag }}
{{- end }}

{{- define "nebulous.huggingfaceCachePVCName" -}}
{{- default "huggingface-cache-pvc" .Values.storage.huggingfaceCache.claimName }}
{{- end }}

{{- define "nebulous.adapterPVCName" -}}
{{- default "adapter-pvc" .Values.storage.adapter.claimName }}
{{- end }}

{{- define "nebulous.datasetPVCName" -}}
{{- default "dataset-pvc" .Values.storage.dataset.claimName }}
{{- end }}

{{- define "nebulous.modelPVCName" -}}
{{- default "model-pvc" .Values.storage.model.claimName }}
{{- end }}

{{- define "nebulous.messageQueueType" -}}
{{- if .Values.redis.create }}
{{- print "redis" }}
{{- else }}
{{- default "redis" .Values.messageQueue.type }}
{{- end }}
{{- end }}

{{- define "nebulous.encryptionKeySecretName" -}}
{{- if .Values.encryptionKey.create }}
{{- $secretName := printf "%s-secret" .Release.Name }}
{{- default $secretName .Values.encryptionKey.secretName }}
{{- else }}
{{- required ".Values.encryptionKey.secretName is required" .Values.encryptionKey.secretName }}
{{- end }}
{{- end }}

{{- define "nebulous.encryptionKeySecretKey" -}}
{{- if .Values.encryptionKey.create }}
{{- default "encryption_key" .Values.encryptionKey.secretKey }}
{{- else }}
{{- required ".Values.encryptionKey.secretKey is required" .Values.encryptionKey.secretKey }}
{{- end }}
{{- end }}

{{- define "postgres.serviceName" -}}
postgres
{{- end }}

{{- define "postgres.host" -}}
{{- if .Values.postgres.create }}
{{- include "postgres.serviceName" . }}.{{- include "nebulous.namespace" . }}.svc.cluster.local
{{- else }}
{{- required ".Values.postgres.host is required" .Values.postgres.host }}
{{- end }}
{{- end }}

{{- define "postgres.port" -}}
{{- default 5432 .Values.postgres.port }}
{{- end }}

{{- define "postgres.user" -}}
{{- if .Values.postgres.create }}
{{- default "nebulous" .Values.postgres.auth.user }}
{{- else }}
{{- required ".Values.postgres.user is required" .Values.postgres.auth.user }}
{{- end }}
{{- end }}

{{- define "postgres.password" -}}
{{- if .Values.postgres.create }}
{{- default "nebulous" .Values.postgres.auth.password }}
{{- else }}
{{- required ".Values.postgres.password is required" .Values.postgres.auth.password }}
{{- end }}
{{- end }}

{{- define "postgres.database" -}}
{{- if .Values.postgres.create }}
{{- default "nebulous" .Values.postgres.database }}
{{- else }}
{{- required ".Values.postgres.database is required" .Values.postgres.database }}
{{- end }}
{{- end }}

{{- define "redis.serviceName" -}}
redis
{{- end }}

{{- define "redis.host" -}}
{{- if .Values.redis.create }}
{{- include "redis.serviceName" . }}.{{- include "nebulous.namespace" . }}.svc.cluster.local
{{- else }}
{{- required ".Values.redis.host is required" .Values.redis.host }}
{{- end }}
{{- end }}

{{- define "redis.port" -}}
{{- default 6379 .Values.redis.port }}
{{- end }}

{{- define "redis.database" -}}
{{- default 0 .Values.redis.database }}
{{- end }}

{{- define "redis.password" -}}
{{- if .Values.redis.create }}
{{- default "nebulous" .Values.redis.auth.password }}
{{- else }}
{{- required ".Values.redis.password is required" .Values.redis.auth.password }}
{{- end }}
{{- end }}

{{- define "providers.aws.secretName" -}}
{{- default "aws-secret" .Values.providers.aws.secret.name }}
{{- end }}

{{- define "providers.aws.accessKeyIdSecretKey" -}}
{{- default "AWS_ACCESS_KEY_ID" .Values.providers.aws.secret.keys.accessKeyId }}
{{- end }}

{{- define "providers.aws.secretAccessKeySecretKey" -}}
{{- default "AWS_SECRET_ACCESS_KEY" .Values.providers.aws.secret.keys.secretAccessKey }}
{{- end }}

{{- define "providers.runpod.secretName" -}}
{{- default "runpod-secret" .Values.providers.runpod.secret.name }}
{{- end }}

{{- define "providers.runpod.apiKeySecretKey" -}}
{{- default "RUNPOD_API_KEY" .Values.providers.runpod.secret.keys.apiKey }}
{{- end }}
