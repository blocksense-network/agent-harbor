/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import Navbar from './components/layout/Navbar';
import Footer from './components/layout/Footer';
import Hero from './components/sections/Hero';
import Features from './components/sections/Features';
import Benefits from './components/sections/Benefits';
import EarlyAccess from './components/sections/EarlyAccess';

export default function Home() {
  return (
    <>
      <div className="fixed inset-0 bg-grid-pattern pointer-events-none z-0"></div>
      <div className="fixed inset-0 bg-gradient-to-b from-gray-950 via-transparent to-gray-950 pointer-events-none z-0"></div>

      <Navbar />
      <Hero />
      <Features />
      <Benefits />
      <EarlyAccess />
      <Footer />
    </>
  );
}
