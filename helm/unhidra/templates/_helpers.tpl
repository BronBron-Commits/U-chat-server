{{/*
Expand the name of the chart.
*/}}
{{- define "unhidra.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "unhidra.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "unhidra.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "unhidra.labels" -}}
helm.sh/chart: {{ include "unhidra.chart" . }}
{{ include "unhidra.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- with .Values.commonLabels }}
{{ toYaml . }}
{{- end }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "unhidra.selectorLabels" -}}
app.kubernetes.io/name: {{ include "unhidra.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "unhidra.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "unhidra.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Auth API fullname
*/}}
{{- define "unhidra.authApi.fullname" -}}
{{- printf "%s-auth-api" (include "unhidra.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Gateway Service fullname
*/}}
{{- define "unhidra.gatewayService.fullname" -}}
{{- printf "%s-gateway" (include "unhidra.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Chat Service fullname
*/}}
{{- define "unhidra.chatService.fullname" -}}
{{- printf "%s-chat" (include "unhidra.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Redis connection URL
*/}}
{{- define "unhidra.redisUrl" -}}
{{- if .Values.redis.enabled }}
{{- printf "redis://%s-redis-master:6379" .Release.Name }}
{{- else }}
{{- .Values.externalRedis.url }}
{{- end }}
{{- end }}

{{/*
PostgreSQL connection URL
*/}}
{{- define "unhidra.postgresUrl" -}}
{{- if .Values.postgresql.enabled }}
{{- printf "postgres://%s:%s@%s-postgresql:5432/%s" .Values.postgresql.auth.username "$(POSTGRES_PASSWORD)" .Release.Name .Values.postgresql.auth.database }}
{{- else }}
{{- .Values.externalPostgresql.url }}
{{- end }}
{{- end }}
