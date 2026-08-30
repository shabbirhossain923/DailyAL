  }

  static String get defaultConfig {
    return '''
        {
          "bmacLink": false,
          "errorLogging": false,
          "maxLoad": 20,
          "preferredServers": [],
          "strategy": "load"
        }
''';
  }

  static String get supabaseUrl {
    return '${environment['SUPABASE_URL']}';
  }

  static String get supabaseKey {
    return '${environment['SUPABASE_KEY']}';
  }

  /// DAL API base URL used by graph/review/image endpoints.
  /// Older builds could turn a missing API_URL into the literal string
  /// "null", which made graph requests go to an invalid host. Keep the
  /// deployed API as a safe fallback while still allowing API_URL to override it.
  static String get apiURL {
    final rawConfigured = environment['API_URL'];
    final configured = rawConfigured is String ? rawConfigured.trim() : null;
    if (configured == null || configured.isEmpty || configured == 'null') {
      return 'https://dailyal-s3ym.onrender.com';
    }
    return configured.replaceFirst(RegExp(r'/+$'), '');
  }

  static String get apiSecret {
    return '${environment['API_SECRET']}';
  }
}