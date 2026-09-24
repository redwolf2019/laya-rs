def probability: type == "number" and . >= 0 and . <= 1;
def distribution: type == "object" and length > 0 and all(.[]; probability)
  and (([.[]] | add) as $sum | $sum >= 0.999 and $sum <= 1.001);
.model == "rl-agent"
and .answers.department.type == "choice"
and .answers.priority.type == "score"
and .answers.urgent.type == "noul"
and (.answers.department.choice as $choice | ["billing", "technical", "sales"] | index($choice) != null)
and (.answers.department.probabilities | distribution)
and (.answers.priority.score | type == "number" and . >= 0 and . <= 3)
and (.answers.priority.probabilities | distribution)
and (.answers.urgent.noul | probability)
and (.usage.input_tokens | type == "number" and . > 0)
and .usage.output_tokens == 0
