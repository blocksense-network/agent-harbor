/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

'use client';

import { FormEvent } from 'react';

export default function EarlyAccess() {
  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    // Handle form submission
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

        <div className="bg-gray-900 border border-gray-800 rounded-2xl p-8 shadow-2xl relative overflow-hidden group">
          <div className="absolute -top-24 -right-24 w-48 h-48 bg-brand/10 rounded-full blur-3xl group-hover:bg-brand/20 transition-all duration-700"></div>

          <form onSubmit={handleSubmit} className="space-y-6 relative z-10">
            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">Name</label>
              <input
                type="text"
                placeholder="Your name"
                className="w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:border-brand focus:ring-1 focus:ring-brand outline-none transition-colors"
              />
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">Email</label>
              <input
                type="email"
                placeholder="Your email"
                className="w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:border-brand focus:ring-1 focus:ring-brand outline-none transition-colors"
              />
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                What is your Github?
              </label>
              <input
                type="text"
                placeholder="Github project URL"
                className="w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:border-brand focus:ring-1 focus:ring-brand outline-none transition-colors"
              />
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                What is your project&apos;s name?
              </label>
              <input
                type="text"
                placeholder="Project name"
                className="w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:border-brand focus:ring-1 focus:ring-brand outline-none transition-colors"
              />
            </div>

            <div>
              <label className="block text-sm font-mono text-gray-400 mb-2">
                What is your role in your company?
              </label>
              <input
                type="text"
                placeholder="ex. CTO"
                className="w-full bg-gray-950 border border-gray-700 rounded-lg px-4 py-3 text-white placeholder-gray-600 focus:border-brand focus:ring-1 focus:ring-brand outline-none transition-colors"
              />
            </div>

            <button
              type="submit"
              className="w-full bg-brand text-black font-mono font-bold text-lg py-4 rounded-lg hover:bg-brand-hover transition-all shadow-[0_0_20px_rgba(0,255,247,0.3)] hover:shadow-[0_0_30px_rgba(0,255,247,0.5)] mt-4"
            >
              Submit Application
            </button>
          </form>
        </div>
      </div>
    </section>
  );
}
