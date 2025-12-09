# Code Health: how it works

Code Health is an aggregated metric based on 25+ factors scanned from the source code. The code health factors correlate with increased maintenance costs and an increased risk for defects. The code health factors cover three levels of abstraction: 
1. Module level: is the class/module/file cohesive and well-organized?
2. Function level: checks the modularity, encapsulation, and cohesiveness of individual functions.
3. Implementation level: does the code introduce accidental complexity, ie. is it implemented in a more complex way than the problem calls for?

Code Health is calulcated at the file-level where each file is given a composite score of 1.0–10.0. Higher is better.
CodeScene also aggregates the file scores into composite scores for architectural components (services, packages, sub-systems), as well 
as into high-level KPIs for the whole codebase.

The Code Health scores are categorized and interpreted as:
* Optimal Code: a Code Health 10.0 is optimized for both human and AI comprehension
* Green Code: high quality with a score of 9-9.9
* Yellow Code: problematic techncial debt with a score of 4-8.9
* Red Code: severe techncial debt, maintainability issues, and expensive onboarding with a score of 1.0-3.9