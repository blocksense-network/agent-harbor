/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

/**
 * Sanitize string input by trimming and removing control characters
 */
export function sanitizeString(input: string, maxLength: number): string {
  return (
    input
      .trim()
      // eslint-disable-next-line no-control-regex -- Control characters need to be removed for security
      .replace(/[\x00-\x1F\x7F]/g, '') // Remove control characters
      .slice(0, maxLength)
  );
}

/**
 * Validate and sanitize email address
 */
export function sanitizeEmail(email: string): string {
  const trimmed = email.trim().toLowerCase();
  // Basic email validation regex
  const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
  if (!emailRegex.test(trimmed)) {
    throw new Error('Invalid email format');
  }
  if (trimmed.length > 254) {
    throw new Error('Email address is too long');
  }
  return trimmed;
}

/**
 * Validate and sanitize URL
 */
export function sanitizeUrl(url: string): string {
  const trimmed = url.trim();
  if (!trimmed) {
    return '';
  }

  try {
    const urlObj = new URL(trimmed);
    // Only allow http and https protocols
    if (!['http:', 'https:'].includes(urlObj.protocol)) {
      throw new Error('URL must use http or https protocol');
    }
    if (trimmed.length > 2048) {
      throw new Error('URL is too long');
    }
    return trimmed;
  } catch (error) {
    if (error instanceof TypeError) {
      throw new Error('Invalid URL format');
    }
    throw error;
  }
}

/**
 * Validate that a value is one of the allowed options
 */
export function validateEnum<T extends string>(value: string, allowedValues: readonly T[]): T {
  if (!allowedValues.includes(value as T)) {
    throw new Error(`Invalid value. Must be one of: ${allowedValues.join(', ')}`);
  }
  return value as T;
}

/**
 * Validate string length
 */
export function validateLength(
  input: string,
  minLength: number,
  maxLength: number,
  fieldName: string,
): string {
  if (input.length < minLength) {
    throw new Error(`${fieldName} must be at least ${minLength} characters`);
  }
  if (input.length > maxLength) {
    throw new Error(`${fieldName} must be no more than ${maxLength} characters`);
  }
  return input;
}

/**
 * Sanitize form data before submission
 */
export interface FormData {
  name: string;
  email: string;
  primaryRole: string;
  codeProfileUrls?: string[];
  affiliation: string;
  organizationName?: string;
}

const PRIMARY_ROLES = [
  'Engineering Manager / Tech Lead',
  'Professional Developer (Company/Agency)',
  'Freelancer / Indie Developer',
  'Open Source Maintainer',
  'Student / Researcher',
  'Hobbyist',
] as const;

const AFFILIATIONS = [
  'Company / startup',
  'University / school',
  'Independent / not affiliated',
  'Prefer not to say',
] as const;

export function sanitizeFormData(data: FormData): FormData {
  // Validate and sanitize code profile URLs if provided (optional field)
  const sanitizedUrls: string[] = [];
  if (data.codeProfileUrls && data.codeProfileUrls.length > 0) {
    for (const url of data.codeProfileUrls) {
      const trimmed = url.trim();
      if (trimmed.length > 0) {
        try {
          sanitizedUrls.push(sanitizeUrl(trimmed));
        } catch (error) {
          if (error instanceof Error) {
            throw new Error(`Invalid code profile URL: ${error.message}`);
          }
          throw new Error(
            'Invalid code profile URL format. URLs must start with http:// or https://',
          );
        }
      }
    }
  }

  return {
    name: validateLength(sanitizeString(data.name, 100), 1, 100, 'Name'),
    email: sanitizeEmail(data.email),
    primaryRole: validateEnum(data.primaryRole, PRIMARY_ROLES),
    codeProfileUrls: sanitizedUrls.length > 0 ? sanitizedUrls : undefined,
    affiliation: validateEnum(data.affiliation, AFFILIATIONS),
    organizationName: data.organizationName
      ? validateLength(sanitizeString(data.organizationName, 200), 1, 200, 'Organization name')
      : undefined,
  };
}
