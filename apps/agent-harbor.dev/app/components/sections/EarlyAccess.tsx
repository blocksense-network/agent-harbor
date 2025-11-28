/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

'use client';

import { FormEvent, useState } from 'react';
import { Dropdown } from '../ui/Dropdown';
import { InfoIcon } from '../ui/InfoIcon';
import { sanitizeFormData } from '../../utils/validation';

interface FieldErrors {
  name?: string;
  email?: string;
  primaryRole?: string;
  codeProfileUrls?: string;
  affiliation?: string;
  organizationName?: string;
}

export default function EarlyAccess() {
  const [formData, setFormData] = useState({
    name: '',
    email: '',
    primaryRole: '',
    codeProfileUrls: [''],
    affiliation: '',
    organizationName: '',
  });
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({});
  const [submitStatus, setSubmitStatus] = useState<{
    type: 'success' | 'error' | null;
    message: string;
  }>({ type: null, message: '' });

  /**
   * Parse error message to extract field-specific errors
   */
  const parseError = (error: Error): { message: string; fieldErrors: FieldErrors } => {
    const message = error.message;
    const fieldErrors: FieldErrors = {};

    // Check for common validation error patterns
    if (message.includes('Name')) {
      fieldErrors.name = message;
    } else if (message.includes('email') || message.includes('Email')) {
      fieldErrors.email = message;
    } else if (
      message.includes('URL') ||
      message.includes('url') ||
      message.includes('code profile')
    ) {
      fieldErrors.codeProfileUrls = message;
    } else if (message.includes('Organization')) {
      fieldErrors.organizationName = message;
    } else if (message.includes('role') || message.includes('Role')) {
      fieldErrors.primaryRole = message;
    } else if (message.includes('affiliation') || message.includes('Affiliation')) {
      fieldErrors.affiliation = message;
    }

    return { message, fieldErrors };
  };

  /**
   * Get user-friendly error message
   */
  const getErrorMessage = (error: unknown, status?: number): string => {
    if (error instanceof Error) {
      // Network errors
      if (error.message.includes('fetch') || error.message.includes('network')) {
        return 'Network error. Please check your internet connection and try again.';
      }

      // Validation errors
      if (error.message.includes('Invalid') || error.message.includes('must be')) {
        return error.message;
      }

      // Email already registered
      if (error.message.includes('already registered') || error.message.includes('Conflict')) {
        return 'This email address is already registered. Please use a different email or contact support if you believe this is an error.';
      }

      return error.message;
    }

    // HTTP status code errors
    if (status === 400) {
      return 'Invalid form data. Please check your inputs and try again.';
    }
    if (status === 401 || status === 403) {
      return 'Authentication failed. Please refresh the page and try again.';
    }
    if (status === 409) {
      return 'This email address is already registered. Please use a different email.';
    }
    if (status === 500) {
      return 'Server error. Please try again later or contact support if the problem persists.';
    }
    if (status && status >= 500) {
      return 'Server error. Please try again later.';
    }

    return 'Failed to submit application. Please try again.';
  };

  const handleSubmit = async (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setIsSubmitting(true);
    setSubmitStatus({ type: null, message: '' });
    setFieldErrors({});

    // Client-side validation before submission
    const errors: FieldErrors = {};
    if (!formData.name.trim()) {
      errors.name = 'Name is required';
    }
    if (!formData.email.trim()) {
      errors.email = 'Email is required';
    }
    if (!formData.primaryRole) {
      errors.primaryRole = 'Please select your primary role';
    }
    if (!formData.affiliation) {
      errors.affiliation = 'Please select your affiliation';
    }
    // Validate code profile URLs if provided (optional field)
    const validCodeProfileUrls = formData.codeProfileUrls
      .map((url: string) => url.trim())
      .filter((url: string) => url.length > 0);
    if (validCodeProfileUrls.length > 0) {
      // Validate that each URL is a valid URL format
      for (const url of validCodeProfileUrls) {
        try {
          const urlObj = new URL(url);
          if (!['http:', 'https:'].includes(urlObj.protocol)) {
            errors.codeProfileUrls = 'URLs must start with http:// or https://';
            break;
          }
        } catch {
          errors.codeProfileUrls =
            'Invalid URL format. Please enter a complete URL starting with http:// or https://';
          break;
        }
      }
    }
    if (
      (formData.affiliation === 'Company / startup' ||
        formData.affiliation === 'University / school') &&
      !formData.organizationName?.trim()
    ) {
      errors.organizationName = 'Organization name is required';
    }

    // If there are client-side validation errors, show them and stop
    if (Object.keys(errors).length > 0) {
      setFieldErrors(errors);
      setSubmitStatus({
        type: 'error',
        message: 'Please fill in all required fields correctly.',
      });
      setIsSubmitting(false);
      return;
    }

    try {
      // Process code profile URLs - filter out empty strings and validate URLs
      const codeProfileUrls = formData.codeProfileUrls
        .map((url: string) => url.trim())
        .filter((url: string) => url.length > 0)
        .map((url: string) => {
          // Validate URL format
          try {
            const urlObj = new URL(url);
            if (!['http:', 'https:'].includes(urlObj.protocol)) {
              throw new Error('URL must use http:// or https:// protocol');
            }
            return url;
          } catch {
            // If URL parsing fails, try prepending https://
            if (!url.includes('://')) {
              try {
                const urlWithProtocol = `https://${url}`;
                new URL(urlWithProtocol); // Validate the URL
                return urlWithProtocol;
              } catch {
                throw new Error(
                  'Invalid URL format. Please enter a complete URL starting with http:// or https://',
                );
              }
            }
            throw new Error(
              'Invalid URL format. Please enter a complete URL starting with http:// or https://',
            );
          }
        });

      // Sanitize and validate form data before submission
      const sanitizedData = sanitizeFormData({
        name: formData.name,
        email: formData.email,
        primaryRole: formData.primaryRole,
        codeProfileUrls: codeProfileUrls.length > 0 ? codeProfileUrls : undefined,
        affiliation: formData.affiliation,
        organizationName: formData.organizationName,
      });

      // Use production worker URL from environment variable, fallback to localhost for development
      const workerUrl = process.env.NEXT_PUBLIC_WORKER_URL || 'http://localhost:8787';

      const response = await fetch(`${workerUrl}/api/submit`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'x-api-key': process.env.NEXT_PUBLIC_API_KEY || '',
        },
        body: JSON.stringify(sanitizedData),
      });

      if (!response.ok) {
        let errorData: { message?: string; errors?: Record<string, string[]> } = {};
        try {
          errorData = await response.json();
        } catch {
          // If response is not JSON, use status code
        }

        // Handle Effect Schema validation errors
        if (errorData.errors) {
          const errors: FieldErrors = {};
          Object.entries(errorData.errors).forEach(([field, messages]) => {
            const fieldName = field
              .toLowerCase()
              .replace(/([A-Z])/g, '_$1')
              .toLowerCase();
            errors[fieldName as keyof FieldErrors] = Array.isArray(messages)
              ? messages[0]
              : messages;
          });
          setFieldErrors(errors);
          throw new Error('Please fix the errors below and try again.');
        }

        const errorMessage =
          errorData.message || getErrorMessage(new Error('Request failed'), response.status);
        throw new Error(errorMessage);
      }

      await response.json();
      setSubmitStatus({
        type: 'success',
        message: 'Thank you! Your application has been submitted successfully.',
      });
      // Reset form
      setFormData({
        name: '',
        email: '',
        primaryRole: '',
        codeProfileUrls: [''],
        affiliation: '',
        organizationName: '',
      });
      setFieldErrors({});
    } catch (error) {
      const { message, fieldErrors: parsedFieldErrors } =
        error instanceof Error
          ? parseError(error)
          : { message: getErrorMessage(error), fieldErrors: {} };

      setFieldErrors(parsedFieldErrors);
      setSubmitStatus({
        type: 'error',
        message,
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <section id="early-access" className="relative z-10 py-24 pb-32">
      <div className="max-w-2xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="text-center mb-10">
          <h2 className="text-3xl font-bold text-white mb-6">Join the Early Access Program</h2>
          <p className="text-gray-400 leading-relaxed mb-4">
            If you are an avid <span className="text-brand font-mono font-bold">vibecoder</span> and
            would like to be part of a selective community of early testers, you&apos;re in the
            right place.
          </p>
          <p className="text-sm text-gray-500 max-w-lg mx-auto">
            Get a direct line to the Agent Harbor team for support, see under the hood before public
            launch, score exclusive swag, and help us build the best product for you.
          </p>
        </div>

        <div className="bg-gray-900 border border-gray-800 rounded-2xl p-8 shadow-2xl relative overflow-visible group">
          <div className="hidden sm:block absolute -top-24 -right-24 w-48 h-48 bg-brand/10 rounded-full blur-3xl group-hover:bg-brand/20 transition-all duration-700"></div>

          <form onSubmit={handleSubmit} className="space-y-6 relative z-10 font-mono" noValidate>
            {submitStatus.type && (
              <div
                className={`p-4 rounded-lg flex items-start gap-3 ${
                  submitStatus.type === 'success'
                    ? 'bg-green-900/50 border border-green-700 text-green-300'
                    : 'bg-red-900/50 border border-red-700 text-red-300'
                }`}
                role="alert"
              >
                {submitStatus.type === 'error' && (
                  <svg className="w-5 h-5 shrink-0 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                    <path
                      fillRule="evenodd"
                      d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z"
                      clipRule="evenodd"
                    />
                  </svg>
                )}
                {submitStatus.type === 'success' && (
                  <svg className="w-5 h-5 shrink-0 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                    <path
                      fillRule="evenodd"
                      d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z"
                      clipRule="evenodd"
                    />
                  </svg>
                )}
                <div className="flex-1">
                  <p className="font-medium font-mono">{submitStatus.message}</p>
                </div>
              </div>
            )}

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                Name
                <InfoIcon text="Personalization in future emails." />
              </label>
              <input
                type="text"
                placeholder="How should we address you?"
                value={formData.name}
                onChange={e => {
                  setFormData({ ...formData, name: e.target.value });
                  if (fieldErrors.name) {
                    setFieldErrors({ ...fieldErrors, name: undefined });
                  }
                }}
                required
                className={`w-full bg-gray-950 border rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:ring-1 outline-none transition-colors font-mono ${
                  fieldErrors.name
                    ? 'border-red-600 focus:border-red-500 focus:ring-red-500'
                    : 'border-gray-700 focus:border-brand focus:ring-brand'
                }`}
                aria-invalid={!!fieldErrors.name}
                aria-describedby={fieldErrors.name ? 'name-error' : undefined}
              />
              {fieldErrors.name && (
                <p
                  id="name-error"
                  className="mt-1 text-sm text-red-400 flex items-center gap-1 font-mono"
                >
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path
                      fillRule="evenodd"
                      d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z"
                      clipRule="evenodd"
                    />
                  </svg>
                  {fieldErrors.name}
                </p>
              )}
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                Email Address
                <InfoIcon text="We prefer work emails if you're evaluating for your team, but personal emails are great too." />
              </label>
              <input
                type="email"
                placeholder="Where can we reach you with early-access updates?"
                value={formData.email}
                onChange={e => {
                  setFormData({ ...formData, email: e.target.value });
                  if (fieldErrors.email) {
                    setFieldErrors({ ...fieldErrors, email: undefined });
                  }
                }}
                required
                className={`w-full bg-gray-950 border rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:ring-1 outline-none transition-colors font-mono ${
                  fieldErrors.email
                    ? 'border-red-600 focus:border-red-500 focus:ring-red-500'
                    : 'border-gray-700 focus:border-brand focus:ring-brand'
                }`}
                aria-invalid={!!fieldErrors.email}
                aria-describedby={fieldErrors.email ? 'email-error' : undefined}
              />
              {fieldErrors.email && (
                <p
                  id="email-error"
                  className="mt-1 text-sm text-red-400 flex items-center gap-1 font-mono"
                >
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path
                      fillRule="evenodd"
                      d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z"
                      clipRule="evenodd"
                    />
                  </svg>
                  {fieldErrors.email}
                </p>
              )}
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                What best describes you?
                <InfoIcon text='Marketing segmentation. You speak differently to a "Student" than you do to a "VP of Engineering."' />
              </label>
              <Dropdown
                value={formData.primaryRole}
                onChange={value => {
                  setFormData({ ...formData, primaryRole: value });
                  if (fieldErrors.primaryRole) {
                    setFieldErrors({ ...fieldErrors, primaryRole: undefined });
                  }
                }}
                options={[
                  'Engineering Manager / Tech Lead',
                  'Professional Developer (Company/Agency)',
                  'Freelancer / Indie Developer',
                  'Open Source Maintainer',
                  'Student / Researcher',
                  'Hobbyist',
                ]}
                placeholder="Select your role"
                required
              />
              {fieldErrors.primaryRole && (
                <p className="mt-1 text-sm text-red-400 flex items-center gap-1 font-mono">
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path
                      fillRule="evenodd"
                      d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z"
                      clipRule="evenodd"
                    />
                  </svg>
                  {fieldErrors.primaryRole}
                </p>
              )}
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                Code hosting platform profile URL (Optional)
                <InfoIcon text="Add full URLs (starting with http:// or https://) to your profiles on code hosting platforms like GitHub, GitLab, etc." />
              </label>
              <div className="space-y-2">
                {formData.codeProfileUrls.map((url: string, index: number) => {
                  const placeholders = [
                    'https://github.com/username',
                    'https://gitlab.com/username',
                    'https://bitbucket.org/username',
                    'https://codeberg.org/username',
                    'https://sr.ht/~username',
                    'https://gitea.com/username',
                    'https://gogs.io/username',
                    'https://codecommit.aws.amazon.com/username',
                    'https://radicle.xyz/username',
                    'https://code.launchpad.net/~username',
                  ];
                  const placeholder =
                    index < placeholders.length
                      ? placeholders[index]
                      : 'https://example.com/username';
                  return (
                    <div key={index} className="flex gap-2 overflow-hidden">
                      <input
                        type="url"
                        placeholder={placeholder}
                        value={url}
                        onChange={e => {
                          const newUrls = [...formData.codeProfileUrls];
                          newUrls[index] = e.target.value;
                          setFormData({ ...formData, codeProfileUrls: newUrls });
                          if (fieldErrors.codeProfileUrls) {
                            setFieldErrors({ ...fieldErrors, codeProfileUrls: undefined });
                          }
                        }}
                        className={`min-w-0 flex-1 bg-gray-950 border rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:ring-1 outline-none transition-colors font-mono ${
                          fieldErrors.codeProfileUrls
                            ? 'border-red-600 focus:border-red-500 focus:ring-red-500'
                            : 'border-gray-700 focus:border-brand focus:ring-brand'
                        }`}
                        aria-invalid={!!fieldErrors.codeProfileUrls && index === 0}
                        aria-describedby={
                          fieldErrors.codeProfileUrls && index === 0
                            ? 'code-profile-error'
                            : undefined
                        }
                      />
                      {index === formData.codeProfileUrls.length - 1 && (
                        <button
                          type="button"
                          onClick={() => {
                            setFormData({
                              ...formData,
                              codeProfileUrls: [...formData.codeProfileUrls, ''],
                            });
                          }}
                          className="shrink-0 px-2 sm:px-4 py-3 bg-gray-800 border border-gray-700 rounded-lg text-white font-mono text-xs sm:text-sm hover:bg-gray-700 hover:border-gray-600 transition-colors whitespace-nowrap"
                        >
                          Add more
                        </button>
                      )}
                      {formData.codeProfileUrls.length > 1 && (
                        <button
                          type="button"
                          onClick={() => {
                            const newUrls = formData.codeProfileUrls.filter(
                              (_: string, i: number) => i !== index,
                            );
                            setFormData({ ...formData, codeProfileUrls: newUrls });
                          }}
                          className="shrink-0 px-2 sm:px-4 py-3 bg-gray-800 border border-gray-700 rounded-lg text-white font-mono text-sm hover:bg-gray-700 hover:border-gray-600 transition-colors"
                          aria-label="Remove this field"
                        >
                          <svg
                            className="w-4 h-4 sm:w-5 sm:h-5"
                            fill="currentColor"
                            viewBox="0 0 20 20"
                          >
                            <path
                              fillRule="evenodd"
                              d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z"
                              clipRule="evenodd"
                            />
                          </svg>
                        </button>
                      )}
                    </div>
                  );
                })}
              </div>
              {fieldErrors.codeProfileUrls && (
                <p
                  id="code-profile-error"
                  className="mt-2 text-sm text-red-400 flex items-center gap-1 font-mono"
                >
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path
                      fillRule="evenodd"
                      d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z"
                      clipRule="evenodd"
                    />
                  </svg>
                  {fieldErrors.codeProfileUrls}
                </p>
              )}
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                Are you affiliated with a company, organization, or school?
              </label>
              <Dropdown
                value={formData.affiliation}
                onChange={value => {
                  setFormData({
                    ...formData,
                    affiliation: value,
                    organizationName:
                      value === 'Company / startup' || value === 'University / school'
                        ? formData.organizationName
                        : '',
                  });
                  if (fieldErrors.affiliation) {
                    setFieldErrors({ ...fieldErrors, affiliation: undefined });
                  }
                }}
                options={[
                  'Company / startup',
                  'University / school',
                  'Independent / not affiliated',
                  'Prefer not to say',
                ]}
                placeholder="Select your affiliation"
                required
              />
              {fieldErrors.affiliation && (
                <p className="mt-1 text-sm text-red-400 flex items-center gap-1 font-mono">
                  <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                    <path
                      fillRule="evenodd"
                      d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z"
                      clipRule="evenodd"
                    />
                  </svg>
                  {fieldErrors.affiliation}
                </p>
              )}
              {(formData.affiliation === 'Company / startup' ||
                formData.affiliation === 'University / school') && (
                <div className="mt-3">
                  <input
                    type="text"
                    placeholder={
                      formData.affiliation === 'Company / startup'
                        ? 'Company or startup name'
                        : 'University or school name'
                    }
                    value={formData.organizationName}
                    onChange={e => {
                      setFormData({ ...formData, organizationName: e.target.value });
                      if (fieldErrors.organizationName) {
                        setFieldErrors({ ...fieldErrors, organizationName: undefined });
                      }
                    }}
                    className={`w-full bg-gray-950 border rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:ring-1 outline-none transition-colors font-mono ${
                      fieldErrors.organizationName
                        ? 'border-red-600 focus:border-red-500 focus:ring-red-500'
                        : 'border-gray-700 focus:border-brand focus:ring-brand'
                    }`}
                    aria-invalid={!!fieldErrors.organizationName}
                    aria-describedby={fieldErrors.organizationName ? 'org-error' : undefined}
                  />
                  {fieldErrors.organizationName && (
                    <p
                      id="org-error"
                      className="mt-1 text-sm text-red-400 flex items-center gap-1 font-mono"
                    >
                      <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
                        <path
                          fillRule="evenodd"
                          d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7 4a1 1 0 11-2 0 1 1 0 012 0zm-1-9a1 1 0 00-1 1v4a1 1 0 102 0V6a1 1 0 00-1-1z"
                          clipRule="evenodd"
                        />
                      </svg>
                      {fieldErrors.organizationName}
                    </p>
                  )}
                </div>
              )}
            </div>

            <button
              type="submit"
              disabled={isSubmitting}
              className="w-full bg-brand text-black font-mono font-bold text-lg py-4 rounded-lg hover:bg-brand-hover transition-all shadow-[0_0_20px_rgba(0,255,247,0.3)] hover:shadow-[0_0_30px_rgba(0,255,247,0.5)] mt-4 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {isSubmitting ? 'Submitting...' : 'Submit'}
            </button>
          </form>
        </div>
      </div>
    </section>
  );
}
