const CSRF_COOKIE_NAMES = ['__Host-gruxi_admin_csrf']
const CSRF_HEADER_NAME = 'X-CSRF-Token'

const isStateChangingMethod = (method) => {
  const normalizedMethod = method.toUpperCase()
  return normalizedMethod === 'POST' || normalizedMethod === 'PUT' || normalizedMethod === 'PATCH' || normalizedMethod === 'DELETE'
}

const getCookieValue = (cookieNames) => {
  for (const cookieName of cookieNames) {
    const cookiePrefix = `${cookieName}=`

    for (const cookie of document.cookie.split(';')) {
      const trimmedCookie = cookie.trim()
      if (trimmedCookie.startsWith(cookiePrefix)) {
        return decodeURIComponent(trimmedCookie.slice(cookiePrefix.length))
      }
    }
  }

  return ''
}

const primeCsrfCookie = async () => {
  await fetch('/basic', {
    method: 'GET',
    credentials: 'same-origin'
  })
}

export const apiFetch = async (input, init = {}) => {
  const method = (init.method || 'GET').toUpperCase()
  const headers = new Headers(init.headers || {})

  if (init.body !== undefined && !(init.body instanceof FormData) && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }

  if (isStateChangingMethod(method)) {
    let csrfToken = getCookieValue(CSRF_COOKIE_NAMES)
    if (!csrfToken) {
      await primeCsrfCookie()
      csrfToken = getCookieValue(CSRF_COOKIE_NAMES)
    }
    if (csrfToken) {
      headers.set(CSRF_HEADER_NAME, csrfToken)
    }
  }

  return fetch(input, {
    ...init,
    method,
    credentials: 'same-origin',
    headers
  })
}