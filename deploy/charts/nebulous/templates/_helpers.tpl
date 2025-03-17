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

{{- define "nebulous.serviceName" }}
{{- default .Release.Name .Values.service.name }}
{{- end }}

{{- define "nebulous.servicePort" }}
{{- default 3000 .Values.service.port }}
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

{{- define "nebulous.messageQueueType" }}
{{- if .Values.redis.create }}
redis
{{- else }}
{{- default "redis" .Values.messageQueue.type }}
{{- end }}
{{- end }}

{{- define "nebulous.encryptionKeySecretName" }}
{{- if .Values.encryptionKey.create }}
{{- $secretName := printf "%s-secret" .Release.Name }}
{{- default $secretName .Values.encryptionKey.secretName }}
{{- else }}
{{- .Values.encryptionKey.secretName }}
{{- end }}
{{- end }}

{{- define "nebulous.encryptionKeySecretKey" }}
{{- if .Values.encryptionKey.create }}
{{- default "encryption_key" .Values.encryptionKey.secretKey }}
{{- else }}
{{- .Values.encryptionKey.secretKey }}
{{- end }}
{{- end }}

{{- define "postgres.serviceName" }}
postgres
{{- end }}

{{- define "postgres.host" }}
{{- if .Values.postgres.create }}
{{- include "postgres.serviceName" . }}.{{- include "nebulous.namespace" . }}.svc.cluster.local
{{- else }}
{{- .Values.postgres.host }}
{{- end }}
{{- end }}

{{- define "postgres.port" }}
{{- default 5432 .Values.postgres.port }}
{{- end }}

{{- define "postgres.user" }}
{{- if .Values.postgres.create }}
{{- default "nebulous" .Values.postgres.user }}
{{- else }}
{{- .Values.postgres.user }}
{{- end }}
{{- end }}

{{- define "postgres.password" }}
{{- if .Values.postgres.create }}
{{- default "nebulous" .Values.postgres.password }}
{{- else }}
{{- .Values.postgres.password }}
{{- end }}
{{- end }}

{{- define "postgres.database" }}
{{- if .Values.postgres.create }}
{{ default "nebulous" .Values.postgres.database }}
{{- else }}
{{- .Values.postgres.database }}
{{- end }}
{{- end }}

{{- define "redis.serviceName" }}
redis
{{- end }}

{{- define "redis.host" }}
{{- if .Values.redis.create }}
{{- include "redis.serviceName" . }}.{{- include "nebulous.namespace" . }}.svc.cluster.local
{{- else }}
{{- .Values.redis.host }}
{{- end }}
{{- end }}

{{- define "redis.port" }}
{{- default 6379 .Values.redis.port }}
{{- end }}

{{- define "redis.password" }}
{{- if .Values.redis.create }}
{{- default "nebulous" .Values.redis.password }}
{{- else }}
{{- .Values.redis.password }}
{{- end }}
{{- end }}
