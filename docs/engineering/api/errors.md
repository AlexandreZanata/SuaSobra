# API Error Response Contracts

All error responses from the Sua Sobra API conform to a single, strict, standardized JSON contract defined across all `/api/v1/` endpoints.

## Structured Error Shape

Every error response returns an object with a root `error` key containing:

```json
{
  "error": {
    "code": "validation_error",
    "message": "Validation failed for one or more fields.",
    "correlation_id": "1894d3f5e1284a71",
    "fields": {
      "quantity": [
        "Quantity must be a positive integer."
      ]
    }
  }
}
```

### Fields Specification

| Field | Type | Description |
|---|---|---|
| `code` | string | Machine-readable stable error code (snake_case). |
| `message` | string | Human-readable description of the error condition. |
| `correlation_id` | string | Unique trace identifier; also returned in the `X-Correlation-ID` response header. |
| `fields` | object | Key-value mapping of field names to string arrays of specific validation issues. Empty object `{}` when error is not field-specific. |

## Standard Error Codes

| HTTP Status | Error Code | Description |
|---|---|---|
| 400 | `validation_error` | Request payload failed schema or business rule validation. |
| 400 | `unknown_field` | Request contained unexpected or unknown writable fields. |
| 401 | `authentication_required` | Endpoint requires authentication and none was provided. |
| 401 | `authentication_failed` | Provided authentication credentials were invalid. |
| 403 | `permission_denied` | Authenticated actor lacks required permission/role for the operation. |
| 404 | `not_found` | Requested resource does not exist or is inaccessible. |
| 405 | `method_not_allowed` | HTTP method is not permitted on this resource. |
| 409 | `conflict` | Request conflicts with current resource state or concurrency version. |
| 429 | `rate_limit_exceeded` | Request rate limit has been exceeded. |
| 500 | `server_error` | Unexpected internal server error. |

## Traceback and Security Guarantees

1. **No Internal Traceback Leakage**: Internal Rust stack traces, filenames, line numbers, and database driver details are never serialized into responses.
2. **Correlation Tracking**: Each unhandled failure is logged internally alongside its `correlation_id`, allowing operations to correlate client reports with server logs.
3. **Header Mirroring**: The `X-Correlation-ID` header is attached to every error response.

Resource IDs are UUID strings. Malformed numeric/UUID IDs return validation_error rather than being treated as proof of access. Deferred email routes return HTTP 501 with code feature_deferred; disabled financial routes are absent. Request bodies over 64 KiB return a structured HTTP 413 error.
