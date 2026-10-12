// System JavaScript for Automation, used only to validate JSON data; no application automation.
ObjC.import('Foundation');
function run(argv) {
    const text = $.NSString.stringWithContentsOfFileEncodingError(argv[0], $.NSUTF8StringEncoding, null);
    const r = JSON.parse(ObjC.unwrap(text));
    const probability = v => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= 1;
    const distribution = v => {
        if (!v || typeof v !== 'object' || Array.isArray(v)) return false;
        const values = Object.keys(v).map(k => v[k]);
        return values.length > 0 && values.every(probability) && Math.abs(values.reduce((a, b) => a + b, 0) - 1) <= 0.001;
    };
    const a = r.answers;
    if (!(r.model === 'rl-agent' && r.usage.input_tokens > 0 && r.usage.output_tokens === 0 &&
        a.department.type === 'choice' && ['billing', 'technical', 'sales'].includes(a.department.choice) &&
        distribution(a.department.probabilities) && a.priority.type === 'score' &&
        typeof a.priority.score === 'number' && a.priority.score >= 0 && a.priority.score <= 3 &&
        distribution(a.priority.probabilities) && a.urgent.type === 'noul' && probability(a.urgent.noul))) {
        throw new Error('Invalid three-type inference response');
    }
    return 'PASS: Chinese Choice/Score/Noul real API response';
}
